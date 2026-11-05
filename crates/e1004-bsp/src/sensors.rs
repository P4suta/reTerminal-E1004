use device_core::{
    geometry::{LOW_NIBBLE_MASK, NIBBLE_BITS},
    telemetry::{DateTime, Fault, Humidity, Temperature, HUMIDITY_FULL_SCALE, RTC_YEAR_START},
};

const CRC_INITIAL: u8 = 0xff;
const CRC_TOP_BIT: u8 = 0x80;
const CRC_POLYNOMIAL: u8 = 0x31;
const BITS_PER_BYTE: usize = 8;
const TEMPERATURE_OFFSET: i32 = -4500;
const TEMPERATURE_SCALE: i32 = 17500;
const HUMIDITY_OFFSET: i32 = -600;
const HUMIDITY_SCALE: i32 = 12500;
const SENSOR_FULL_SCALE: i32 = 65535;
const TEMPERATURE_BYTES: [usize; 2] = [0, 1];
const TEMPERATURE_CRC: usize = 2;
const HUMIDITY_BYTES: [usize; 2] = [3, 4];
const HUMIDITY_CRC: usize = 5;
const DECIMAL_BASE: u8 = 10;
const DECIMAL_DIGIT_MAX: u8 = 9;
const RTC_STATUS_FLAG: u8 = 0x80;
const RTC_YEAR: usize = 6;
const RTC_MONTH: usize = 5;
const RTC_DAY: usize = 3;
const RTC_WEEKDAY: usize = 4;
const RTC_HOUR: usize = 2;
const RTC_MINUTE: usize = 1;
const RTC_SECOND: usize = 0;
const RTC_WEEKDAY_MAX: u8 = 6;
const RTC_MONTH_MASK: u8 = 0x1f;
const RTC_DAY_HOUR_MASK: u8 = 0x3f;
const RTC_MINUTE_SECOND_MASK: u8 = 0x7f;
const RTC_SECONDS_REGISTER: u8 = 2;

pub fn sensor_crc(bytes: [u8; 2]) -> u8 {
    let mut crc = CRC_INITIAL;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..BITS_PER_BYTE {
            crc = if crc & CRC_TOP_BIT != 0 {
                (crc << 1) ^ CRC_POLYNOMIAL
            } else {
                crc << 1
            };
        }
    }
    crc
}

pub fn sht40_decode(bytes: [u8; 6]) -> Result<(Temperature, Humidity), Fault> {
    let temperature_word = [bytes[TEMPERATURE_BYTES[0]], bytes[TEMPERATURE_BYTES[1]]];
    let humidity_word = [bytes[HUMIDITY_BYTES[0]], bytes[HUMIDITY_BYTES[1]]];
    if sensor_crc(temperature_word) != bytes[TEMPERATURE_CRC]
        || sensor_crc(humidity_word) != bytes[HUMIDITY_CRC]
    {
        return Err(Fault::SensorCrc);
    }
    let raw_t = i32::from(u16::from_be_bytes(temperature_word));
    let raw_h = i32::from(u16::from_be_bytes(humidity_word));
    let temperature = Temperature::new(
        (TEMPERATURE_OFFSET + TEMPERATURE_SCALE * raw_t / SENSOR_FULL_SCALE) as i16,
    )
    .ok_or(Fault::InvalidInput)?;
    let humidity = Humidity::new(
        (HUMIDITY_OFFSET + HUMIDITY_SCALE * raw_h / SENSOR_FULL_SCALE)
            .clamp(0, i32::from(HUMIDITY_FULL_SCALE)) as u16,
    )
    .ok_or(Fault::InvalidInput)?;
    Ok((temperature, humidity))
}

fn bcd(value: u8) -> Result<u8, Fault> {
    if value & LOW_NIBBLE_MASK > DECIMAL_DIGIT_MAX || value >> NIBBLE_BITS > DECIMAL_DIGIT_MAX {
        return Err(Fault::InvalidRtc);
    }
    Ok((value >> NIBBLE_BITS) * DECIMAL_BASE + (value & LOW_NIBBLE_MASK))
}

pub fn rtc_decode(bytes: [u8; 7]) -> Result<DateTime, Fault> {
    if bytes[RTC_SECOND] & RTC_STATUS_FLAG != 0
        || bytes[RTC_MONTH] & RTC_STATUS_FLAG != 0
        || bytes[RTC_WEEKDAY] > RTC_WEEKDAY_MAX
    {
        return Err(Fault::InvalidRtc);
    }
    let time = DateTime {
        year: RTC_YEAR_START + u16::from(bcd(bytes[RTC_YEAR])?),
        month: bcd(bytes[RTC_MONTH] & RTC_MONTH_MASK)?,
        day: bcd(bytes[RTC_DAY] & RTC_DAY_HOUR_MASK)?,
        hour: bcd(bytes[RTC_HOUR] & RTC_DAY_HOUR_MASK)?,
        minute: bcd(bytes[RTC_MINUTE] & RTC_MINUTE_SECOND_MASK)?,
        second: bcd(bytes[RTC_SECOND] & RTC_MINUTE_SECOND_MASK)?,
    };
    if !time.valid() {
        return Err(Fault::InvalidRtc);
    }
    Ok(time)
}

fn to_bcd(value: u8) -> u8 {
    ((value / DECIMAL_BASE) << NIBBLE_BITS) | (value % DECIMAL_BASE)
}

pub fn rtc_encode(time: DateTime) -> Result<[u8; 8], Fault> {
    if !time.valid() {
        return Err(Fault::InvalidRtc);
    }
    Ok([
        RTC_SECONDS_REGISTER,
        to_bcd(time.second),
        to_bcd(time.minute),
        to_bcd(time.hour),
        to_bcd(time.day),
        0,
        to_bcd(time.month),
        to_bcd((time.year - RTC_YEAR_START) as u8),
    ])
}
