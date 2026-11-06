const DECIMAL_INPUT_BYTES: usize = 8;
const DECIMAL_RADIX: i32 = 10;
const HUNDREDTHS_PER_UNIT: i32 = 100;
const MAX_DECIMAL_PLACES: u8 = 2;

pub fn parse_hundredths(input: &str) -> Option<i32> {
    if input.is_empty() || input.len() > DECIMAL_INPUT_BYTES {
        return None;
    }
    let (negative, digits) = match input.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, input),
    };
    let mut value = 0i32;
    let mut has_whole_digit = false;
    let mut fractional_digits = None;
    for byte in digits.bytes() {
        if byte == b'.' {
            if !has_whole_digit || fractional_digits.is_some() {
                return None;
            }
            fractional_digits = Some(0);
        } else if byte.is_ascii_digit() {
            value = value
                .checked_mul(DECIMAL_RADIX)?
                .checked_add(i32::from(byte - b'0'))?;
            if let Some(places) = fractional_digits.as_mut() {
                *places += 1;
                if *places > MAX_DECIMAL_PLACES {
                    return None;
                }
            } else {
                has_whole_digit = true;
            }
        } else {
            return None;
        }
    }
    if !has_whole_digit || fractional_digits == Some(0) {
        return None;
    }
    let scaled = match fractional_digits {
        None => value.checked_mul(HUNDREDTHS_PER_UNIT)?,
        Some(1) => value.checked_mul(DECIMAL_RADIX)?,
        Some(_) => value,
    };
    Some(if negative { -scaled } else { scaled })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryMillivolts(u16);

impl BatteryMillivolts {
    pub fn new(value: u16) -> Option<Self> {
        BATTERY_MILLIVOLT_RANGE
            .contains(&value)
            .then_some(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub fn estimated_percent(self) -> u8 {
        if self.0 <= EMPTY_BATTERY_MILLIVOLTS {
            return 0;
        }
        for pair in BATTERY_ANCHORS.windows(2) {
            let (low_mv, low_percent) = pair[0];
            let (high_mv, high_percent) = pair[1];
            if self.0 <= high_mv {
                return (low_percent
                    + (self.0 - low_mv) * (high_percent - low_percent) / (high_mv - low_mv))
                    as u8;
            }
        }
        FULL_BATTERY_PERCENT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Temperature(i16);

impl Temperature {
    pub fn new(centidegrees: i16) -> Option<Self> {
        TEMPERATURE_CENTIDEGREE_RANGE
            .contains(&centidegrees)
            .then_some(Self(centidegrees))
    }

    pub const fn get(self) -> i16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Humidity(u16);

impl Humidity {
    pub fn new(hundredths: u16) -> Option<Self> {
        (hundredths <= HUMIDITY_FULL_SCALE).then_some(Self(hundredths))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    SensorCrc,
    Bus,
    PanelTimeout,
    Storage,
    SettingsStorage,
    InvalidRtc,
    InvalidInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample<T> {
    Unavailable,
    Missing,
    Fault(Fault),
    Fresh { value: T, measured_at_ms: u64 },
    Stale { value: T, measured_at_ms: u64 },
}

impl<T: Copy> Sample<T> {
    pub fn at(self, now_ms: u64, max_age_ms: u64) -> Self {
        match self {
            Self::Fresh {
                value,
                measured_at_ms,
            }
            | Self::Stale {
                value,
                measured_at_ms,
            } => {
                if now_ms >= measured_at_ms && now_ms - measured_at_ms <= max_age_ms {
                    Self::Fresh {
                        value,
                        measured_at_ms,
                    }
                } else {
                    Self::Stale {
                        value,
                        measured_at_ms,
                    }
                }
            }
            Self::Unavailable | Self::Missing | Self::Fault(_) => self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charging {
    Unknown,
    Charging,
    NotCharging,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageStatus {
    Absent,
    Ready,
    Fault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioStatus {
    Off,
    Disconnected,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeReason {
    PowerOn,
    Timer,
    Button,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != ISO_DATE_TIME_BYTES {
            return None;
        }
        if text
            .bytes()
            .enumerate()
            .any(|(index, byte)| !ISO_SEPARATORS.contains(&index) && !byte.is_ascii_digit())
        {
            return None;
        }
        if text.get(ISO_YEAR_SEPARATOR)? != "-"
            || text.get(ISO_MONTH_SEPARATOR)? != "-"
            || text.get(ISO_TIME_SEPARATOR)? != "T"
            || text.get(ISO_HOUR_SEPARATOR)? != ":"
            || text.get(ISO_MINUTE_SEPARATOR)? != ":"
        {
            return None;
        }
        let time = Self {
            year: text.get(ISO_YEAR)?.parse().ok()?,
            month: text.get(ISO_MONTH)?.parse().ok()?,
            day: text.get(ISO_DAY)?.parse().ok()?,
            hour: text.get(ISO_HOUR)?.parse().ok()?,
            minute: text.get(ISO_MINUTE)?.parse().ok()?,
            second: text.get(ISO_SECOND)?.parse().ok()?,
        };
        time.valid().then_some(time)
    }
    pub fn valid(self) -> bool {
        let days = match self.month {
            JANUARY | MARCH | MAY | JULY | AUGUST | OCTOBER | DECEMBER => LONG_MONTH_DAYS,
            APRIL | JUNE | SEPTEMBER | NOVEMBER => SHORT_MONTH_DAYS,
            FEBRUARY if self.year.is_multiple_of(LEAP_YEAR_PERIOD) => LEAP_FEBRUARY_DAYS,
            FEBRUARY => FEBRUARY_DAYS,
            _ => return false,
        };
        (RTC_YEAR_START..=RTC_YEAR_END).contains(&self.year)
            && self.day >= 1
            && self.day <= days
            && self.hour < HOURS_PER_DAY
            && self.minute < MINUTES_PER_HOUR
            && self.second < SECONDS_PER_MINUTE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    pub clock: Sample<DateTime>,
    pub temperature: Sample<Temperature>,
    pub humidity: Sample<Humidity>,
    pub battery: Sample<BatteryMillivolts>,
    pub charging: Charging,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        Self {
            clock: Sample::Unavailable,
            temperature: Sample::Unavailable,
            humidity: Sample::Unavailable,
            battery: Sample::Unavailable,
            charging: Charging::Unknown,
        }
    }
}
const BATTERY_MILLIVOLT_RANGE: core::ops::RangeInclusive<u16> = 2000..=5000;
const EMPTY_BATTERY_MILLIVOLTS: u16 = 3300;
const FULL_BATTERY_PERCENT: u8 = 100;
const BATTERY_ANCHORS: [(u16, u16); 7] = [
    (3300, 0),
    (3600, 10),
    (3700, 30),
    (3800, 55),
    (3900, 75),
    (4000, 90),
    (4200, 100),
];
const TEMPERATURE_CENTIDEGREE_RANGE: core::ops::RangeInclusive<i16> = -4500..=13000;
pub const HUMIDITY_FULL_SCALE: u16 = 10000;
pub const RTC_YEAR_START: u16 = 2000;
const RTC_YEAR_END: u16 = 2099;
const HOURS_PER_DAY: u8 = 24;
const MINUTES_PER_HOUR: u8 = 60;
const SECONDS_PER_MINUTE: u8 = 60;
const ISO_DATE_TIME_BYTES: usize = 19;
const ISO_SEPARATORS: [usize; 5] = [4, 7, 10, 13, 16];
const ISO_YEAR: core::ops::Range<usize> = 0..4;
const ISO_MONTH: core::ops::Range<usize> = 5..7;
const ISO_DAY: core::ops::Range<usize> = 8..10;
const ISO_HOUR: core::ops::Range<usize> = 11..13;
const ISO_MINUTE: core::ops::Range<usize> = 14..16;
const ISO_SECOND: core::ops::Range<usize> = 17..19;
const ISO_YEAR_SEPARATOR: core::ops::Range<usize> = 4..5;
const ISO_MONTH_SEPARATOR: core::ops::Range<usize> = 7..8;
const ISO_TIME_SEPARATOR: core::ops::Range<usize> = 10..11;
const ISO_HOUR_SEPARATOR: core::ops::Range<usize> = 13..14;
const ISO_MINUTE_SEPARATOR: core::ops::Range<usize> = 16..17;
const JANUARY: u8 = 1;
const FEBRUARY: u8 = 2;
const MARCH: u8 = 3;
const APRIL: u8 = 4;
const MAY: u8 = 5;
const JUNE: u8 = 6;
const JULY: u8 = 7;
const AUGUST: u8 = 8;
const SEPTEMBER: u8 = 9;
const OCTOBER: u8 = 10;
const NOVEMBER: u8 = 11;
const DECEMBER: u8 = 12;
const LONG_MONTH_DAYS: u8 = 31;
const SHORT_MONTH_DAYS: u8 = 30;
const FEBRUARY_DAYS: u8 = 28;
const LEAP_FEBRUARY_DAYS: u8 = 29;
const LEAP_YEAR_PERIOD: u16 = 4;
