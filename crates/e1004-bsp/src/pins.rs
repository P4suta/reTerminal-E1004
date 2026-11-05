pub const PANEL_SCK: u8 = 7;
pub const PANEL_MISO: u8 = 8;
pub const PANEL_MOSI: u8 = 9;
pub const PANEL_LEFT_CS: u8 = 10;
pub const PANEL_RIGHT_CS: u8 = 2;
pub const PANEL_DC: u8 = 11;
pub const PANEL_ENABLE: u8 = 12;
pub const PANEL_BUSY: u8 = 13;
pub const PANEL_RESET: u8 = 38;
pub const BUTTONS: [u8; 3] = [3, 4, 5];
pub const BATTERY_ADC: u8 = 1;
pub const BATTERY_ENABLE: u8 = 21;
pub const I2C_SDA: u8 = 19;
pub const I2C_SCL: u8 = 20;
pub const SD_CS: u8 = 14;
pub const SD_DETECT: u8 = 15;
pub const SD_ENABLE: u8 = 16;
pub const LED: u8 = 48;
pub const BUZZER: u8 = 45;
pub const SERIAL_RX: u8 = 44;
pub const SERIAL_TX: u8 = 43;
pub const EXTENSION: [u8; 5] = [39, 40, 41, 42, 6];
pub const RESERVED: [u8; 28] = [
    PANEL_SCK,
    PANEL_MISO,
    PANEL_MOSI,
    PANEL_LEFT_CS,
    PANEL_RIGHT_CS,
    PANEL_DC,
    PANEL_ENABLE,
    PANEL_BUSY,
    PANEL_RESET,
    BUTTONS[0],
    BUTTONS[1],
    BUTTONS[2],
    BATTERY_ADC,
    BATTERY_ENABLE,
    I2C_SDA,
    I2C_SCL,
    SD_CS,
    SD_DETECT,
    SD_ENABLE,
    LED,
    BUZZER,
    SERIAL_RX,
    SERIAL_TX,
    EXTENSION[0],
    EXTENSION[1],
    EXTENSION[2],
    EXTENSION[3],
    EXTENSION[4],
];

pub const fn unique(pins: &[u8]) -> bool {
    let mut i = 0;
    while i < pins.len() {
        let mut j = i + 1;
        while j < pins.len() {
            if pins[i] == pins[j] {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

const _: () = assert!(unique(&RESERVED));
