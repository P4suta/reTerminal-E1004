pub const WIDTH: u16 = 1200;
pub const HEIGHT: u16 = 1600;
pub const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize / 2;
pub const NIBBLE_BITS: u32 = 4;
pub const LOW_NIBBLE_MASK: u8 = 0x0f;
pub const HIGH_NIBBLE_MASK: u8 = 0xf0;
pub const PACKED_WHITE: u8 = 0x11;
pub const CONTROLLER_ROW_BYTES: u16 = WIDTH / 2 / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    Portrait,
    Landscape,
    InvertedPortrait,
    InvertedLandscape,
}

impl Rotation {
    pub const fn dimensions(self) -> (u16, u16) {
        match self {
            Self::Portrait | Self::InvertedPortrait => (WIDTH, HEIGHT),
            Self::Landscape | Self::InvertedLandscape => (HEIGHT, WIDTH),
        }
    }

    pub fn physical(self, x: u16, y: u16) -> Option<(u16, u16)> {
        let (width, height) = self.dimensions();
        if x >= width || y >= height {
            return None;
        }
        Some(match self {
            Self::Portrait => (x, y),
            Self::Landscape => (y, HEIGHT - 1 - x),
            Self::InvertedPortrait => (WIDTH - 1 - x, HEIGHT - 1 - y),
            Self::InvertedLandscape => (WIDTH - 1 - y, x),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    White = 1,
    Yellow = 2,
    Red = 3,
    Blue = 5,
    Green = 6,
}

pub fn pixel_address(x: u16, y: u16) -> Option<(usize, bool)> {
    if x >= WIDTH || y >= HEIGHT {
        return None;
    }
    Some((
        usize::from(y) * usize::from(WIDTH / 2) + usize::from(x / 2),
        x.is_multiple_of(2),
    ))
}

pub fn replace_nibble(byte: u8, high: bool, color: Color) -> u8 {
    if high {
        (byte & LOW_NIBBLE_MASK) + ((color as u8) << NIBBLE_BITS)
    } else {
        (byte & HIGH_NIBBLE_MASK) + color as u8
    }
}

pub fn controller_source(right: bool, row: u16, column_byte: u16) -> Option<usize> {
    if row >= HEIGHT || column_byte >= CONTROLLER_ROW_BYTES {
        return None;
    }
    Some(
        usize::from(row) * usize::from(WIDTH / 2)
            + usize::from(column_byte)
            + if right {
                usize::from(CONTROLLER_ROW_BYTES)
            } else {
                0
            },
    )
}
