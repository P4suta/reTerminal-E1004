#![no_std]

use core::{
    convert::Infallible,
    fmt::{self, Write},
};
use device_core::{
    app::MILLISECONDS_PER_SECOND,
    app::{App, ScreenId},
    geometry::{pixel_address, replace_nibble, Color, Rotation, FRAME_BYTES, HEIGHT},
    telemetry::*,
};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::{BinaryColor, Rgb888, RgbColor},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};

mod daily;
mod layout;
use daily::{CalendarScreen, ChecklistScreen, NotesScreen};
#[cfg(kani)]
mod proofs;
use layout::*;

const REPEATED_NIBBLE: u8 = 0x11;
const LUMINANCE_WEIGHTS: [u32; 3] = [299, 587, 114];
const WHITE_THRESHOLD: u32 = 128_000;
const HUNDREDTHS_PER_UNIT: i32 = 100;
const TENTHS_PER_UNIT: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    BufferLength,
    InvalidBitmap,
}

pub struct Framebuffer<'a> {
    bytes: &'a mut [u8],
    rotation: Rotation,
}

impl<'a> Framebuffer<'a> {
    pub fn new(bytes: &'a mut [u8], rotation: Rotation) -> Result<Self, DrawError> {
        if bytes.len() != FRAME_BYTES {
            return Err(DrawError::BufferLength);
        }
        Ok(Self { bytes, rotation })
    }
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
    pub fn put(&mut self, x: u16, y: u16, color: Color) {
        if let Some((px, py)) = self.rotation.physical(x, y) {
            if let Some((index, high)) = pixel_address(px, py) {
                if let Some(byte) = self.bytes.get_mut(index) {
                    *byte = replace_nibble(*byte, high, color);
                }
            }
        }
    }
    pub fn fill(&mut self, color: Color) {
        self.bytes.fill((color as u8) * REPEATED_NIBBLE);
    }
    pub fn dimensions(&self) -> (u16, u16) {
        self.rotation.dimensions()
    }
}

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        let (w, h) = self.dimensions();
        Size::new(u32::from(w), u32::from(h))
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = BinaryColor;
    type Error = Infallible;
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if let (Ok(x), Ok(y)) = (u16::try_from(point.x), u16::try_from(point.y)) {
                self.put(
                    x,
                    y,
                    if color == BinaryColor::On {
                        Color::Black
                    } else {
                        Color::White
                    },
                );
            }
        }
        Ok(())
    }
}

pub trait Screen {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App);
}
pub struct DeviceStatus;
pub struct DiagnosticsScreen;
pub struct DrawingScreen;

pub fn render(app: &App, target: &mut Framebuffer<'_>) {
    render_with_media(app, target, None);
}

pub fn render_with_media(
    app: &App,
    target: &mut Framebuffer<'_>,
    image: Option<device_core::media::PackedFrame<'_>>,
) {
    target.fill(Color::White);
    if app.screen == ScreenId::Gallery {
        if let Some(image) = image {
            let (sw, sh) = image.rotation().dimensions();
            let (w, h) = target.dimensions();
            if let (Some(source), Some(destination)) = (
                device_core::image::Size::new(sw, sh),
                device_core::image::Size::new(w, h),
            ) {
                let settings = app.gallery.settings();
                let placement = device_core::image::Placement::new(
                    source,
                    destination,
                    settings.fit,
                    settings.anchor,
                );
                for y in 0..h {
                    for x in 0..w {
                        if let Some((sx, sy)) = placement.source(x, y) {
                            if let Some(color) = image.pixel(sx, sy) {
                                target.put(x, y, color);
                            }
                        }
                    }
                }
            }
            return;
        }
    }
    match app.screen {
        ScreenId::Status => DeviceStatus.draw(target, app),
        ScreenId::Diagnostics => DiagnosticsScreen.draw(target, app),
        ScreenId::Drawing => DrawingScreen.draw(target, app),
        ScreenId::Calendar => CalendarScreen.draw(target, app),
        ScreenId::Checklist => ChecklistScreen.draw(target, app),
        ScreenId::Notes => NotesScreen.draw(target, app),
        ScreenId::Gallery => {
            text(target, "GALLERY", MARGIN, 100, HEADING_SCALE);
            text(
                target,
                "Import images in the browser.",
                MARGIN,
                200,
                LABEL_SCALE,
            );
            text(
                target,
                "Hold a side button to change screens.",
                MARGIN,
                270,
                FOOTNOTE_SCALE,
            );
        }
    }
    let (w, h) = target.dimensions();
    rule(target, i32::from(h) - FOOTER_RULE_BOTTOM);
    let controls = TextBuffer::<DETAIL_TEXT_BYTES>::formatted(format_args!(
        "PREVIOUS     {}     NEXT",
        app.middle_label()
    ));
    text(
        target,
        controls.as_str(),
        MARGIN,
        i32::from(h) - FOOTER_NAV_BOTTOM,
        LABEL_SCALE,
    );
    let footer = TextBuffer::<DETAIL_TEXT_BYTES>::formatted(format_args!(
        "Refresh every {} s",
        app.refresh_seconds
    ));
    text(
        target,
        footer.as_str(),
        i32::from(w) - FOOTER_NOTE_RIGHT,
        i32::from(h) - FOOTER_NOTE_BOTTOM,
        FOOTNOTE_SCALE,
    );
}

pub fn draw_bmp(
    target: &mut Framebuffer<'_>,
    bytes: &[u8],
    origin: Point,
) -> Result<(), DrawError> {
    let bitmap = tinybmp::Bmp::<Rgb888>::from_slice(bytes).map_err(|_| DrawError::InvalidBitmap)?;
    for Pixel(point, rgb) in bitmap.pixels() {
        if let (Some(x), Some(y)) = (
            point
                .x
                .checked_add(origin.x)
                .and_then(|x| u16::try_from(x).ok()),
            point
                .y
                .checked_add(origin.y)
                .and_then(|y| u16::try_from(y).ok()),
        ) {
            let luminance = u32::from(rgb.r()) * LUMINANCE_WEIGHTS[0]
                + u32::from(rgb.g()) * LUMINANCE_WEIGHTS[1]
                + u32::from(rgb.b()) * LUMINANCE_WEIGHTS[2];
            target.put(
                x,
                y,
                if luminance >= WHITE_THRESHOLD {
                    Color::White
                } else {
                    Color::Black
                },
            );
        }
    }
    Ok(())
}

const BITMAP_SIDE: usize = 128;
const BITMAP_HEADER: usize = 54;
const BITMAP_BYTES: usize = BITMAP_HEADER + BITMAP_SIDE * BITMAP_SIDE * 3;
pub static DEMO_BMP: [u8; BITMAP_BYTES] = {
    let mut bmp = [0; BITMAP_BYTES];
    bmp[0] = b'B';
    bmp[1] = b'M';
    let length = (BITMAP_BYTES as u32).to_le_bytes();
    let data_length = ((BITMAP_BYTES - BITMAP_HEADER) as u32).to_le_bytes();
    let mut i = 0;
    while i < 4 {
        bmp[2 + i] = length[i];
        bmp[34 + i] = data_length[i];
        i += 1;
    }
    bmp[10] = BITMAP_HEADER as u8;
    bmp[14] = 40;
    bmp[18] = BITMAP_SIDE as u8;
    bmp[22] = BITMAP_SIDE as u8;
    bmp[26] = 1;
    bmp[28] = 24;
    let mut y = 0;
    while y < BITMAP_SIDE {
        let mut x = 0;
        while x < BITMAP_SIDE {
            let value = if (x / 16 + y / 16) % 2 == 0 { 0 } else { 255 };
            let offset = BITMAP_HEADER + ((BITMAP_SIDE - 1 - y) * BITMAP_SIDE + x) * 3;
            bmp[offset] = value;
            bmp[offset + 1] = value;
            bmp[offset + 2] = value;
            x += 1;
        }
        y += 1;
    }
    bmp
};

struct Scaled<'a, 'b> {
    target: &'a mut Framebuffer<'b>,
    origin: Point,
    scale: i32,
}
impl OriginDimensions for Scaled<'_, '_> {
    fn size(&self) -> Size {
        self.target.size()
    }
}
impl DrawTarget for Scaled<'_, '_> {
    type Color = BinaryColor;
    type Error = Infallible;
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<BinaryColor>>,
    {
        for Pixel(point, color) in pixels {
            for dy in 0..self.scale {
                for dx in 0..self.scale {
                    let x = self.origin.x + point.x * self.scale + dx;
                    let y = self.origin.y + point.y * self.scale + dy;
                    if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                        self.target.put(
                            x,
                            y,
                            if color == BinaryColor::On {
                                Color::Black
                            } else {
                                Color::White
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

fn text(target: &mut Framebuffer<'_>, value: &str, x: i32, y: i32, scale: i32) {
    painted(
        Text::with_baseline(
            value,
            Point::zero(),
            MonoTextStyle::new(&FONT_10X20, BinaryColor::On),
            Baseline::Top,
        )
        .draw(&mut Scaled {
            target,
            origin: Point::new(x, y),
            scale,
        }),
    );
}
fn painted<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}
fn rule(target: &mut Framebuffer<'_>, y: i32) {
    let (w, _) = target.dimensions();
    painted(
        Line::new(Point::new(MARGIN, y), Point::new(i32::from(w) - MARGIN, y))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, RULE_STROKE))
            .draw(target),
    );
}
fn heading(target: &mut Framebuffer<'_>, title: &str) {
    text(target, "reTerminal E1004", MARGIN, BRAND_Y, LABEL_SCALE);
    text(target, title, MARGIN, HEADING_Y, HEADING_SCALE);
    rule(target, HEADING_RULE_Y);
}
fn sample_state<T>(value: &Sample<T>) -> &'static str {
    match value {
        Sample::Unavailable => "NOT MEASURED",
        Sample::Missing => "NOT PRESENT",
        Sample::Fault(_) => "FAULT",
        Sample::Fresh { .. } => "FRESH",
        Sample::Stale { .. } => "STALE",
    }
}
fn sample_value<T: Copy>(value: Sample<T>) -> Option<T> {
    match value {
        Sample::Fresh { value, .. } | Sample::Stale { value, .. } => Some(value),
        _ => None,
    }
}

impl Screen for DeviceStatus {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App) {
        heading(target, "Device Status");
        let (w, h) = target.dimensions();
        let middle = i32::from(w) / 2;
        let compact = h < HEIGHT;
        let metric_y = if compact {
            METRIC_LANDSCAPE_Y
        } else {
            METRIC_PORTRAIT_Y
        };
        let battery_y = if compact {
            BATTERY_LANDSCAPE_Y
        } else {
            BATTERY_PORTRAIT_Y
        };
        let value = if let Some(clock) = sample_value(app.telemetry.clock) {
            TextBuffer::<STATUS_TEXT_BYTES>::formatted(format_args!(
                "{:02}:{:02}",
                clock.hour, clock.minute
            ))
        } else {
            TextBuffer::formatted(format_args!("--:--"))
        };
        text(target, value.as_str(), MARGIN, TIME_Y, CLOCK_SCALE);
        text(
            target,
            sample_state(&app.telemetry.clock),
            middle + HALF_GUTTER,
            TIME_Y + CLOCK_STATE_OFFSET,
            LABEL_SCALE,
        );
        if let Some(date) = sample_value(app.telemetry.clock).filter(|date| date.valid()) {
            let weekday = device_core::calendar::Month::new(date.year, date.month)
                .and_then(|month| month.weekday(date.day))
                .and_then(|day| day.get(..3))
                .unwrap_or("");
            let date = TextBuffer::<32>::formatted(format_args!(
                "{weekday} {:04}-{:02}-{:02}",
                date.year, date.month, date.day
            ));
            text(
                target,
                date.as_str(),
                middle + HALF_GUTTER,
                TIME_Y + 128,
                LABEL_SCALE,
            );
        }
        text(
            target,
            "TEMPERATURE / C",
            MARGIN,
            metric_y - METRIC_LABEL_ABOVE,
            LABEL_SCALE,
        );
        text(
            target,
            "HUMIDITY / %",
            middle + HALF_GUTTER,
            metric_y - METRIC_LABEL_ABOVE,
            LABEL_SCALE,
        );
        let temperature = if let Some(t) = sample_value(app.telemetry.temperature) {
            let n = i32::from(t.get());
            TextBuffer::<VALUE_TEXT_BYTES>::formatted(format_args!(
                "{}{}.{:01}",
                if n < 0 { "-" } else { "" },
                n.abs() / HUNDREDTHS_PER_UNIT,
                (n.abs() % HUNDREDTHS_PER_UNIT) / TENTHS_PER_UNIT
            ))
        } else {
            TextBuffer::formatted(format_args!("--.-"))
        };
        text(target, temperature.as_str(), MARGIN, metric_y, METRIC_SCALE);
        let humidity = if let Some(h) = sample_value(app.telemetry.humidity) {
            TextBuffer::<VALUE_TEXT_BYTES>::formatted(format_args!(
                "{}.{:01}",
                h.get() / HUNDREDTHS_PER_UNIT as u16,
                (h.get() % HUNDREDTHS_PER_UNIT as u16) / TENTHS_PER_UNIT as u16
            ))
        } else {
            TextBuffer::formatted(format_args!("--.-"))
        };
        text(
            target,
            humidity.as_str(),
            middle + HALF_GUTTER,
            metric_y,
            METRIC_SCALE,
        );
        text(
            target,
            sample_state(&app.telemetry.temperature),
            MARGIN,
            metric_y + METRIC_STATE_OFFSET,
            LABEL_SCALE,
        );
        text(
            target,
            sample_state(&app.telemetry.humidity),
            middle + HALF_GUTTER,
            metric_y + METRIC_STATE_OFFSET,
            LABEL_SCALE,
        );
        rule(target, battery_y - BATTERY_RULE_ABOVE);
        text(target, "BATTERY / ESTIMATE", MARGIN, battery_y, LABEL_SCALE);
        let battery = if let Some(b) = sample_value(app.telemetry.battery) {
            TextBuffer::<DETAIL_TEXT_BYTES>::formatted(format_args!(
                "{}%  {} mV",
                b.estimated_percent(),
                b.get()
            ))
        } else {
            TextBuffer::formatted(format_args!("NOT MEASURED"))
        };
        text(
            target,
            battery.as_str(),
            MARGIN,
            battery_y + BATTERY_VALUE_OFFSET,
            BATTERY_SCALE,
        );
        let charging = TextBuffer::<DETAIL_TEXT_BYTES>::formatted(format_args!(
            "Charging: {}",
            match app.telemetry.charging {
                Charging::Unknown => "Unknown",
                Charging::Charging => "Charging",
                Charging::NotCharging => "Not charging",
            }
        ));
        text(
            target,
            charging.as_str(),
            MARGIN,
            battery_y + CHARGING_OFFSET,
            LABEL_SCALE,
        );
        let measured = match app.telemetry.temperature {
            Sample::Fresh { measured_at_ms, .. } | Sample::Stale { measured_at_ms, .. } => {
                TextBuffer::<STATUS_TEXT_BYTES>::formatted(format_args!(
                    "Measured at uptime {} s",
                    measured_at_ms / MILLISECONDS_PER_SECOND
                ))
            }
            _ => TextBuffer::formatted(format_args!("Measurement time unavailable")),
        };
        text(
            target,
            measured.as_str(),
            MARGIN,
            battery_y + MEASUREMENT_TIME_OFFSET,
            FOOTNOTE_SCALE,
        );
    }
}

impl Screen for DiagnosticsScreen {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App) {
        heading(target, "Diagnostics");
        let d = app.diagnostics;
        let lines: [TextBuffer<DIAGNOSTIC_TEXT_BYTES>; DIAGNOSTIC_LINES] = [
            TextBuffer::formatted(format_args!(
                "Firmware {} / ESP32-S3",
                env!("CARGO_PKG_VERSION")
            )),
            TextBuffer::formatted(format_args!("Wake: {:?}", d.wake)),
            TextBuffer::formatted(format_args!("Heap: {} bytes", d.heap_bytes)),
            TextBuffer::formatted(format_args!("PSRAM: {} bytes", d.psram_bytes)),
            TextBuffer::formatted(format_args!("SD card: {:?}", d.storage)),
            TextBuffer::formatted(format_args!("Wi-Fi: {:?} / BLE: {:?}", d.wifi, d.ble)),
            TextBuffer::formatted(format_args!("Last update: {} ms", d.last_update_ms)),
            TextBuffer::formatted(format_args!("Last error: {:?}", d.last_error)),
            TextBuffer::formatted(format_args!("Interval: {} s", app.refresh_seconds)),
        ];
        for (i, line) in lines.iter().enumerate() {
            text(
                target,
                line.as_str(),
                MARGIN,
                DIAGNOSTIC_FIRST_Y + i as i32 * DIAGNOSTIC_LINE_HEIGHT,
                LABEL_SCALE,
            );
        }
    }
}

impl Screen for DrawingScreen {
    fn draw(&self, target: &mut Framebuffer<'_>, _app: &App) {
        heading(target, "Drawing");
        text(
            target,
            "SHARED RUST GRAPHICS",
            MARGIN,
            DRAW_LABEL_Y,
            DRAW_LABEL_SCALE,
        );
        painted(
            Rectangle::new(DRAW_RECT_ORIGIN, DRAW_RECT_SIZE)
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, DRAW_STROKE))
                .draw(target),
        );
        painted(
            Circle::new(DRAW_CIRCLE_ORIGIN, DRAW_CIRCLE_DIAMETER)
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, DRAW_STROKE))
                .draw(target),
        );
        if draw_bmp(target, &DEMO_BMP, DRAW_BITMAP_ORIGIN).is_ok() {
            text(
                target,
                "BMP",
                DRAW_BITMAP_ORIGIN.x,
                DRAW_BITMAP_LABEL_Y,
                LABEL_SCALE,
            );
        }
        for i in 0..DRAW_LINE_COUNT {
            let y = DRAW_LINE_FIRST_Y + i * DRAW_LINE_HEIGHT;
            painted(
                Line::new(
                    Point::new(DRAW_LINE_LEFT, y),
                    Point::new(DRAW_LINE_RIGHT, y),
                )
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, i as u32 + 1))
                .draw(target),
            );
        }
        text(
            target,
            "Add a Screen implementation",
            DRAW_EXTENSION_LABEL.x,
            DRAW_EXTENSION_LABEL.y,
            LABEL_SCALE,
        );
    }
}

pub struct TextBuffer<const N: usize> {
    bytes: [u8; N],
    len: usize,
    failed: bool,
}
impl<const N: usize> Default for TextBuffer<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            failed: false,
        }
    }
}
impl<const N: usize> TextBuffer<N> {
    pub fn formatted(arguments: fmt::Arguments<'_>) -> Self {
        let mut value = Self::default();
        if value.write_fmt(arguments).is_err() {
            value.failed = true;
        }
        value
    }
    pub fn as_str(&self) -> &str {
        if self.failed {
            "TEXT OVERFLOW"
        } else {
            core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("INVALID TEXT")
        }
    }
    fn append_bytes(&mut self, bytes: &[u8]) -> fmt::Result {
        if bytes.len() > N - self.len {
            self.failed = true;
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}
impl<const N: usize> Write for TextBuffer<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.append_bytes(s.as_bytes())
    }
}
