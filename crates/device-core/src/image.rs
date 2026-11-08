use crate::geometry::{pixel_address, replace_nibble, Color, Rotation, FRAME_BYTES};

pub const MAX_SOURCE_PIXELS: u32 = 16_777_216;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    width: u16,
    height: u16,
}

impl Size {
    pub fn new(width: u16, height: u16) -> Option<Self> {
        (width > 0
            && height > 0
            && width <= 16384
            && height <= 16384
            && u32::from(width) * u32::from(height) <= MAX_SOURCE_PIXELS)
            .then_some(Self { width, height })
    }
    pub const fn width(self) -> u16 {
        self.width
    }
    pub const fn height(self) -> u16 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Fit {
    #[default]
    Contain,
    Cover,
    Original,
    Stretch,
}

impl Fit {
    pub const fn from_code(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Contain),
            1 => Some(Self::Cover),
            2 => Some(Self::Original),
            3 => Some(Self::Stretch),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    #[default]
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    pub const fn from_code(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::TopLeft),
            1 => Some(Self::TopCenter),
            2 => Some(Self::TopRight),
            3 => Some(Self::MiddleLeft),
            4 => Some(Self::Center),
            5 => Some(Self::MiddleRight),
            6 => Some(Self::BottomLeft),
            7 => Some(Self::BottomCenter),
            8 => Some(Self::BottomRight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    source: Size,
    target: Size,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Placement {
    pub fn new(source: Size, target: Size, fit: Fit, anchor: Anchor) -> Self {
        let sw = u32::from(source.width);
        let sh = u32::from(source.height);
        let tw = u32::from(target.width);
        let th = u32::from(target.height);
        let (width, height) = match fit {
            Fit::Original => (sw, sh),
            Fit::Stretch => (tw, th),
            Fit::Contain | Fit::Cover => {
                let by_width = if fit == Fit::Contain {
                    tw * sh <= th * sw
                } else {
                    tw * sh >= th * sw
                };
                if by_width {
                    (tw, (sh * tw / sw).max(1))
                } else {
                    ((sw * th / sh).max(1), th)
                }
            }
        };
        Self {
            source,
            target,
            width,
            height,
            x: (i32::from(target.width) - width as i32) * (anchor as i32 % 3) / 2,
            y: (i32::from(target.height) - height as i32) * (anchor as i32 / 3) / 2,
        }
    }

    pub const fn rectangle(self) -> (i32, i32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }

    pub fn source(self, x: u16, y: u16) -> Option<(u16, u16)> {
        if x >= self.target.width || y >= self.target.height {
            return None;
        }
        let x = u32::try_from(i32::from(x) - self.x).ok()?;
        let y = u32::try_from(i32::from(y) - self.y).ok()?;
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((
            (x as u64 * u64::from(self.source.width) / u64::from(self.width)) as u16,
            (y as u64 * u64::from(self.source.height) / u64::from(self.height)) as u16,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    SixColor,
    Monochrome,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dither {
    None,
    #[default]
    Ordered,
    FloydSteinberg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conversion {
    palette: Palette,
    dither: Dither,
    brightness: i16,
    contrast: u16,
    saturation: u16,
    invert: bool,
}

impl Default for Conversion {
    fn default() -> Self {
        Self {
            palette: Palette::SixColor,
            dither: Dither::Ordered,
            brightness: 0,
            contrast: 100,
            saturation: 100,
            invert: false,
        }
    }
}

fn luminance(rgb: [i32; 3]) -> i32 {
    (rgb[0] * 299 + rgb[1] * 587 + rgb[2] * 114) / 1000
}

impl Conversion {
    pub fn new(
        palette: Palette,
        dither: Dither,
        brightness: i16,
        contrast: u16,
        saturation: u16,
        invert: bool,
    ) -> Option<Self> {
        ((-100..=100).contains(&brightness) && contrast <= 200 && saturation <= 200).then_some(
            Self {
                palette,
                dither,
                brightness,
                contrast,
                saturation,
                invert,
            },
        )
    }

    fn adjusted(self, rgba: [u8; 4]) -> [i32; 3] {
        let alpha = i32::from(rgba[3]);
        let mut rgb = [0; 3];
        for (channel, source) in rgb.iter_mut().zip(rgba) {
            *channel = (i32::from(source) * alpha + 255 * (255 - alpha) + 127) / 255;
        }
        let gray = luminance(rgb);
        rgb.map(|channel| {
            let saturated = gray + (channel - gray) * i32::from(self.saturation) / 100;
            let value = ((saturated - 128) * i32::from(self.contrast) / 100
                + 128
                + i32::from(self.brightness) * 255 / 100)
                .clamp(0, 255);
            if self.invert {
                255 - value
            } else {
                value
            }
        })
    }

    fn nearest(self, rgb: [i32; 3]) -> Color {
        if self.palette == Palette::Monochrome {
            return if luminance(rgb) >= 128 {
                Color::White
            } else {
                Color::Black
            };
        }
        let mut best = Color::Black;
        let mut distance = i32::MAX;
        for color in Color::ALL {
            let value = color.rgb();
            let red = rgb[0] - i32::from(value[0]);
            let green = rgb[1] - i32::from(value[1]);
            let blue = rgb[2] - i32::from(value[2]);
            let candidate = red * red * 299 + green * green * 587 + blue * blue * 114;
            if candidate < distance {
                distance = candidate;
                best = color;
            }
        }
        best
    }

    pub fn pixel(self, rgba: [u8; 4], x: u16, y: u16) -> Color {
        let mut rgb = self.adjusted(rgba);
        if self.dither == Dither::Ordered {
            let offset = i32::from(BAYER[usize::from(y % 8)][usize::from(x % 8)]) * 2 - 63;
            rgb = rgb.map(|channel| (channel + offset).clamp(0, 255));
        }
        self.nearest(rgb)
    }
}

const BAYER: [[u8; 8]; 8] = [
    [0, 48, 12, 60, 3, 51, 15, 63],
    [32, 16, 44, 28, 35, 19, 47, 31],
    [8, 56, 4, 52, 11, 59, 7, 55],
    [40, 24, 36, 20, 43, 27, 39, 23],
    [2, 50, 14, 62, 1, 49, 13, 61],
    [34, 18, 46, 30, 33, 17, 45, 29],
    [10, 58, 6, 54, 9, 57, 5, 53],
    [42, 26, 38, 22, 41, 25, 37, 21],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    Buffer,
    Placement,
}

pub struct RgbaImage<'a> {
    size: Size,
    bytes: &'a [u8],
}

impl<'a> RgbaImage<'a> {
    pub fn new(size: Size, bytes: &'a [u8]) -> Option<Self> {
        (bytes.len() as u64 == u64::from(size.width) * u64::from(size.height) * 4)
            .then_some(Self { size, bytes })
    }

    fn rgba(&self, x: u16, y: u16) -> [u8; 4] {
        let index = (usize::from(y) * usize::from(self.size.width) + usize::from(x)) * 4;
        [
            self.bytes[index],
            self.bytes[index + 1],
            self.bytes[index + 2],
            self.bytes[index + 3],
        ]
    }

    pub fn render(
        &self,
        frame: &mut [u8],
        rotation: Rotation,
        placement: Placement,
        conversion: Conversion,
        errors: &mut [[i32; 3]],
    ) -> Result<(), ImageError> {
        let (width, height) = rotation.dimensions();
        let stride = usize::from(width) + 2;
        let diffusion = conversion.dither == Dither::FloydSteinberg;
        if frame.len() != FRAME_BYTES || (diffusion && errors.len() < 2 * stride) {
            return Err(ImageError::Buffer);
        }
        if placement.source != self.size || placement.target != (Size { width, height }) {
            return Err(ImageError::Placement);
        }
        if diffusion {
            errors.fill([0; 3]);
        }
        frame.fill(0x11);
        for y in 0..height {
            for x in 0..width {
                let Some((sx, sy)) = placement.source(x, y) else {
                    continue;
                };
                let rgba = self.rgba(sx, sy);
                let color = if diffusion {
                    let index = usize::from(x) + 1;
                    let mut rgb = conversion.adjusted(rgba);
                    for channel in 0..3 {
                        rgb[channel] = (rgb[channel] + errors[index][channel] / 16).clamp(0, 255);
                    }
                    let color = conversion.nearest(rgb);
                    let chosen = color.rgb();
                    for channel in 0..3 {
                        let error = rgb[channel] - i32::from(chosen[channel]);
                        errors[index + 1][channel] += error * 7;
                        errors[stride + index - 1][channel] += error * 3;
                        errors[stride + index][channel] += error * 5;
                        errors[stride + index + 1][channel] += error;
                    }
                    color
                } else {
                    conversion.pixel(rgba, x, y)
                };
                if let Some((px, py)) = rotation.physical(x, y) {
                    if let Some((index, high)) = pixel_address(px, py) {
                        frame[index] = replace_nibble(frame[index], high, color);
                    }
                }
            }
            if diffusion {
                let (current, following) = errors[..stride * 2].split_at_mut(stride);
                current.copy_from_slice(following);
                following.fill([0; 3]);
            }
        }
        Ok(())
    }
}
