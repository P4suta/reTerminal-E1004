#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Month {
    year: u16,
    month: u8,
}

pub const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl Month {
    pub fn new(year: u16, month: u8) -> Option<Self> {
        ((2000..=2099).contains(&year) && (1..=12).contains(&month)).then_some(Self { year, month })
    }
    pub const fn year(self) -> u16 {
        self.year
    }
    pub const fn number(self) -> u8 {
        self.month
    }
    pub fn name(self) -> &'static str {
        MONTH_NAMES[usize::from(self.month - 1)]
    }
    pub const fn days(self) -> u8 {
        match self.month {
            4 | 6 | 9 | 11 => 30,
            2 if self.year.is_multiple_of(4) => 29,
            2 => 28,
            _ => 31,
        }
    }
    pub fn first_weekday(self) -> u8 {
        const BEFORE_MONTH: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let years = u32::from(self.year - 2000);
        let leap = u32::from(self.month > 2 && self.year.is_multiple_of(4));
        ((5 + years * 365 + years.div_ceil(4) + BEFORE_MONTH[usize::from(self.month - 1)] + leap)
            % 7) as u8
    }
    pub fn day_at(self, cell: u8) -> Option<u8> {
        let day = cell.checked_sub(self.first_weekday())?.checked_add(1)?;
        (day <= self.days()).then_some(day)
    }
    pub fn weekday(self, day: u8) -> Option<&'static str> {
        if day == 0 || day > self.days() {
            return None;
        }
        Some(WEEKDAYS[usize::from((self.first_weekday() + day - 1) % 7)])
    }
    pub fn step(self, forward: bool) -> Self {
        let index = (self.year - 2000) * 12 + u16::from(self.month - 1);
        let index = if forward {
            index.saturating_add(1).min(1199)
        } else {
            index.saturating_sub(1)
        };
        Self {
            year: 2000 + index / 12,
            month: (index % 12) as u8 + 1,
        }
    }
}
