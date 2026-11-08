use device_core::media::{
    Catalog, GalleryEvent, ImageId, ImageIndex, ImageRecord, PackedFrame, HEADER_BYTES,
    IMAGE_SLOTS, MAX_IMAGES,
};
use device_core::{
    app::*,
    geometry::{Rotation, FRAME_BYTES, LOW_NIBBLE_MASK, NIBBLE_BITS, PACKED_WHITE},
    presentation::Presentation,
    telemetry::*,
};
use device_ui::{render, Framebuffer};
use std::sync::Arc;
mod measurements;
pub use measurements::{InputError, MeasurementInputs};
#[cfg(kani)]
mod proofs;

pub const RGBA_CHANNELS: usize = 4;
const ALPHA_OPAQUE: u8 = 255;
const RGB_BLACK: [u8; 3] = [0, 0, 0];
const RGB_WHITE: [u8; 3] = [255, 255, 255];
const RGB_YELLOW: [u8; 3] = [255, 255, 0];
const RGB_RED: [u8; 3] = [255, 0, 0];
const RGB_BLUE: [u8; 3] = [0, 0, 255];
const RGB_GREEN: [u8; 3] = [0, 255, 0];
const WIRE_BLACK: u8 = device_core::geometry::Color::Black as u8;
const WIRE_YELLOW: u8 = device_core::geometry::Color::Yellow as u8;
const WIRE_RED: u8 = device_core::geometry::Color::Red as u8;
const WIRE_BLUE: u8 = device_core::geometry::Color::Blue as u8;
const WIRE_GREEN: u8 = device_core::geometry::Color::Green as u8;
const MAX_COMMAND_BYTES: usize = 256;
const MAX_EVENTS: usize = 4096;
const MAX_REPLAY_BYTES: usize = 65536;
const MEASUREMENT_FIELD_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayError {
    line: usize,
    message: &'static str,
}
impl ReplayError {
    pub const fn line(self) -> usize {
        self.line
    }
}
impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.line == 0 {
            f.write_str(self.message)
        } else {
            write!(f, "Event log line {}: {}", self.line, self.message)
        }
    }
}
impl std::error::Error for ReplayError {}

#[derive(Clone)]
pub struct Preview {
    app: App,
    immediate: bool,
    events: Vec<String>,
    presentation: Presentation<App>,
    images: [Option<Arc<Vec<u8>>>; MAX_IMAGES],
    shown_frame: Option<Arc<Vec<u8>>>,
    active_frame: Option<(u32, Option<Arc<Vec<u8>>>)>,
}
impl Default for Preview {
    fn default() -> Self {
        Self {
            app: App::default(),
            immediate: true,
            events: Vec::new(),
            presentation: Presentation::new(App::default()),
            images: core::array::from_fn(|_| None),
            shown_frame: None,
            active_frame: None,
        }
    }
}
impl Preview {
    fn catalog(&self) -> Catalog {
        let mut records = [None; IMAGE_SLOTS];
        for (record, image) in records.iter_mut().zip(&self.images) {
            *record = image
                .as_ref()
                .and_then(|bytes| ImageRecord::decode(&bytes[..HEADER_BYTES]));
        }
        ImageIndex::new(records).catalog()
    }
    pub fn import_image(&mut self, number: u8, file: &[u8]) -> Result<(), &'static str> {
        let id = ImageId::new(number).ok_or("Choose an image from 1 to 8.")?;
        let image = PackedFrame::from_file(file).ok_or("Invalid image file or checksum.")?;
        let generation = self
            .catalog()
            .get(id)
            .map_or(Some(0), |value| value.generation.checked_add(1))
            .ok_or("Image generation exhausted.")?;
        let mut bytes = ImageRecord::image(
            id,
            generation,
            image.rotation(),
            device_core::settings::crc32(image.bytes()),
        )
        .encode()
        .to_vec();
        bytes.extend_from_slice(image.bytes());
        self.event(AppEvent::Gallery(GalleryEvent::Busy(true)));
        self.images[id.index()] = Some(Arc::new(bytes));
        self.event(AppEvent::Gallery(GalleryEvent::Committed(
            self.catalog(),
            id,
        )));
        Ok(())
    }
    pub fn export_image(&self, number: u8) -> Option<Vec<u8>> {
        self.images[ImageId::new(number)?.index()]
            .as_ref()
            .map(|bytes| bytes.as_ref().clone())
    }
    pub fn delete_image(&mut self, number: u8) -> Result<(), &'static str> {
        let id = ImageId::new(number).ok_or("Choose an image from 1 to 8.")?;
        self.event(AppEvent::Gallery(GalleryEvent::Busy(true)));
        self.images[id.index()] = None;
        self.event(AppEvent::Gallery(GalleryEvent::Ready(self.catalog())));
        Ok(())
    }
    fn desired_frame(&self) -> Vec<u8> {
        let mut bytes = vec![PACKED_WHITE; FRAME_BYTES];
        let image = self
            .app
            .gallery
            .view()
            .image
            .and_then(|(id, _)| self.images[id.index()].as_ref())
            .and_then(|bytes| PackedFrame::from_file(bytes));
        device_ui::render_with_media(
            &self.app,
            &mut Framebuffer::new(&mut bytes, self.app.rotation).expect("fixed frame size"),
            image,
        );
        bytes
    }
    fn can_record(&self, length: usize) -> bool {
        let bytes = self.events.iter().map(String::len).sum::<usize>()
            + self.events.len().saturating_sub(1);
        recording_fits(bytes, self.events.len(), length)
    }
    fn complete_active(&mut self) {
        if let Some(id) = self.presentation.active_id() {
            self.event(AppEvent::DisplayFinished { id, result: Ok(()) });
        }
    }
    pub const fn app(&self) -> &App {
        &self.app
    }
    pub fn recording(&self) -> String {
        self.events.join("\n")
    }
    pub fn shown(&self) -> (App, Rotation) {
        let app = self.presentation.shown();
        (app, app.rotation)
    }
    pub const fn immediate(&self) -> bool {
        self.immediate
    }

    pub fn apply_measurements(&mut self, inputs: &MeasurementInputs<'_>) -> Result<(), InputError> {
        let command = inputs.command();
        if !self.can_record(command.len()) {
            return Err(InputError::new(
                "events",
                "Event recording is full. Save the recording and reset the preview.",
            ));
        }
        let values = inputs.validate(self.app.now_ms)?;
        self.app.diagnostics.storage = values.sd;
        self.app.diagnostics.wifi = values.wifi;
        self.app.diagnostics.ble = values.ble;
        self.event(AppEvent::Telemetry(values.telemetry));
        self.events.push(command);
        Ok(())
    }

    /// Apply an adapter event.
    /// Use `command` or `apply_measurements` to record preview inputs.
    pub fn event(&mut self, event: AppEvent) {
        let mut next_event = Some(event);
        while let Some(event) = next_event.take() {
            next_event = self.event_step(event);
        }
    }

    fn event_step(&mut self, event: AppEvent) -> Option<AppEvent> {
        if let AppEvent::DisplayFinished { id, result } = event {
            if matches!(self.app.refresh, RefreshState::Updating { id: active, .. } if active == id)
            {
                self.presentation.finish(id, result);
                if self
                    .active_frame
                    .as_ref()
                    .is_some_and(|(active, _)| *active == id)
                {
                    if let Some((_, frame)) = self.active_frame.take() {
                        if result.is_ok() {
                            self.shown_frame = frame;
                        }
                    }
                }
            }
        }
        let transition = self.app.transition(event);
        self.app = transition.state;
        let mut next_event = None;
        for action in transition.actions {
            match action {
                Action::Display { id } => {
                    self.presentation.start(id, self.app);
                    self.active_frame = Some((
                        id,
                        (self.app.screen == ScreenId::Gallery
                            && self.app.gallery.view().image.is_some())
                        .then(|| Arc::new(self.desired_frame())),
                    ));
                }
                Action::StopPanel => {
                    self.presentation.cancel();
                    self.active_frame = None;
                }
                Action::Measure => next_event = Some(AppEvent::Telemetry(self.app.telemetry)),
                Action::None | Action::Sleep { .. } | Action::Log(_) => {}
            }
        }
        if self.immediate && next_event.is_none() {
            next_event = self
                .presentation
                .active_id()
                .map(|id| AppEvent::DisplayFinished { id, result: Ok(()) });
        }
        next_event
    }
    pub fn command(&mut self, command: &str) -> Result<(), &'static str> {
        if !self.can_record(command.len()) {
            return Err("preview event limit exceeded");
        }
        let mut telemetry = self.app.telemetry;
        let (name, value) = command.split_once(' ').unwrap_or((command, ""));
        let measured_at_ms = self.app.now_ms;
        if let Some(command_result) = device_core::commands::parse(command) {
            match command_result?.into_event() {
                Ok(event) => self.event(event),
                Err(rotation) => self.event(AppEvent::Configure(device_core::settings::Settings {
                    rotation: rotation as u8,
                    ..self.app.settings()
                })),
            }
            self.events.push(command.to_owned());
            return Ok(());
        }
        match (name, value) {
            ("measurements", value) => {
                let [clock, temperature, humidity, battery, charging, sd, wifi, ble]: [&str;
                    MEASUREMENT_FIELD_COUNT] = value
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .try_into()
                    .map_err(|_| "expected eight measurement fields")?;
                return self
                    .apply_measurements(&MeasurementInputs {
                        clock,
                        temperature,
                        humidity,
                        battery,
                        charging,
                        sd,
                        wifi,
                        ble,
                    })
                    .map_err(InputError::message);
            }
            ("previous", "") => self.event(AppEvent::Button(Button::Previous)),
            ("next", "") => self.event(AppEvent::Button(Button::Next)),
            ("refresh", "") => self.event(AppEvent::Button(Button::Refresh)),
            ("sleep", "") => self.event(AppEvent::RequestSleep),
            ("wait", "") => {
                self.immediate = false;
            }
            ("immediate", "") => {
                self.immediate = true;
                self.complete_active();
            }
            ("complete", "") | ("timeout", "") => {
                if let RefreshState::Updating { id, .. } = self.app.refresh {
                    self.event(AppEvent::DisplayFinished {
                        id,
                        result: if name == "timeout" {
                            Err(Fault::PanelTimeout)
                        } else {
                            Ok(())
                        },
                    });
                }
            }
            ("tick", value) => self.event(AppEvent::Tick(
                value.parse().map_err(|_| "invalid milliseconds")?,
            )),
            ("sd", value) => self.event(AppEvent::StorageChanged(match value {
                "absent" => StorageStatus::Absent,
                "ready" => StorageStatus::Ready,
                "fault" => StorageStatus::Fault,
                _ => return Err("invalid SD state"),
            })),
            ("wifi", value) | ("ble", value) => {
                let status = match value {
                    "off" => RadioStatus::Off,
                    "disconnected" => RadioStatus::Disconnected,
                    "connected" => RadioStatus::Connected,
                    _ => return Err("invalid radio state"),
                };
                self.event(AppEvent::RadioChanged {
                    wifi: if name == "wifi" {
                        status
                    } else {
                        self.app.diagnostics.wifi
                    },
                    ble: if name == "ble" {
                        status
                    } else {
                        self.app.diagnostics.ble
                    },
                });
            }
            ("temperature", value) => {
                telemetry.temperature =
                    sample_input(value, measured_at_ms, |n| Temperature::new(n.parse().ok()?))?;
                self.event(AppEvent::Telemetry(telemetry));
            }
            ("humidity", value) => {
                telemetry.humidity =
                    sample_input(value, measured_at_ms, |n| Humidity::new(n.parse().ok()?))?;
                self.event(AppEvent::Telemetry(telemetry));
            }
            ("battery", value) => {
                telemetry.battery = sample_input(value, measured_at_ms, |n| {
                    BatteryMillivolts::new(n.parse().ok()?)
                })?;
                self.event(AppEvent::Telemetry(telemetry));
            }
            ("clock", value) => {
                telemetry.clock = sample_input(value, measured_at_ms, DateTime::parse)?;
                self.event(AppEvent::Telemetry(telemetry));
            }
            ("charging", value) => {
                telemetry.charging = match value {
                    "unknown" => Charging::Unknown,
                    "charging" => Charging::Charging,
                    "off" => Charging::NotCharging,
                    _ => return Err("invalid charging state"),
                };
                self.event(AppEvent::Telemetry(telemetry));
            }
            _ => return Err("unknown preview event"),
        }
        self.events.push(command.to_owned());
        Ok(())
    }
    pub fn frame(&self) -> Vec<u8> {
        if let Some(frame) = &self.shown_frame {
            return frame.as_ref().clone();
        }
        let mut bytes = vec![PACKED_WHITE; FRAME_BYTES];
        let (app, rotation) = self.shown();
        render(
            &app,
            &mut Framebuffer::new(&mut bytes, rotation).expect("fixed frame size"),
        );
        bytes
    }
    pub fn replay(&mut self, text: &str) -> Result<(), ReplayError> {
        if text.len() > MAX_REPLAY_BYTES {
            return Err(ReplayError {
                line: 0,
                message: "Event log exceeds 65536 bytes.",
            });
        }
        let mut candidate = Self::default();
        for (index, line) in text.lines().enumerate() {
            if !line.is_empty() {
                candidate.command(line).map_err(|message| ReplayError {
                    line: index + 1,
                    message,
                })?;
            }
        }
        *self = candidate;
        Ok(())
    }
    pub fn text(&self) -> String {
        let t = self.app.telemetry;
        let mut text = format!("{}\nShown screen: {}\nClock: {}\nTemperature: {}\nHumidity: {}\nBattery: {}\nCharging: {}\nSD card: {}\nWi-Fi: {}\nBluetooth Low Energy: {}\n{}\nLast error: {}",
            screen_name(self.app.screen), screen_name(self.shown().0.screen),
            sample_text(t.clock, |d| format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute, d.second)),
            sample_text(t.temperature, |t| format!("{} C", decimal(i32::from(t.get())))),
            sample_text(t.humidity, |h| format!("{} %", decimal(i32::from(h.get())))),
            sample_text(t.battery, |b| format!("{} mV / {}% estimated", b.get(), b.estimated_percent())),
            match t.charging { Charging::Unknown => "unknown", Charging::Charging => "charging", Charging::NotCharging => "not charging" },
            match self.app.diagnostics.storage { StorageStatus::Absent => "absent", StorageStatus::Ready => "ready", StorageStatus::Fault => "fault" },
            radio_name(self.app.diagnostics.wifi), radio_name(self.app.diagnostics.ble), self.display_status(),
            self.app.diagnostics.last_error.map(fault_name).unwrap_or("none"));
        if let Some(month) = self.app.calendar() {
            text.push_str(&format!("\nCalendar: {} {}", month.name(), month.year()));
        }
        let (done, total) = self.app.daily.counts();
        text.push_str(&format!(
            "\nChecklist: {done} of {total} complete\nSelected task: {}",
            self.app.selected_task.number()
        ));
        for (index, task) in self.app.daily.tasks().iter().enumerate() {
            text.push_str(&format!(
                "\n{} [{}] {}",
                index + 1,
                if task.done { "x" } else { " " },
                task.text.as_str()
            ));
        }
        text.push_str(&format!("\nNote: {}", self.app.daily.note().as_str()));
        text
    }

    pub fn display_status(&self) -> &'static str {
        match self.app.refresh {
            RefreshState::Idle => {
                if self.immediate {
                    "Display ready. Updates complete immediately."
                } else {
                    "Display ready. Completion is held for the next update."
                }
            }
            RefreshState::Updating { .. } => {
                if self.app.pending {
                    "Updating. The latest requested screen is queued. Choose Complete update."
                } else {
                    "Updating. The previous image remains visible. Choose Complete update."
                }
            }
            RefreshState::Failed(_) => {
                "Display update failed. The previous image remains visible. Choose Hold middle to retry."
            }
        }
    }
}

fn recording_fits(bytes: usize, events: usize, length: usize) -> bool {
    length <= MAX_COMMAND_BYTES
        && events < MAX_EVENTS
        && bytes
            .checked_add(usize::from(events != 0))
            .and_then(|bytes| bytes.checked_add(length))
            .is_some_and(|bytes| bytes <= MAX_REPLAY_BYTES)
}

fn decimal(value: i32) -> String {
    const HUNDREDTHS: i32 = 100;
    format!(
        "{}{}.{:02}",
        if value < 0 { "-" } else { "" },
        value.abs() / HUNDREDTHS,
        value.abs() % HUNDREDTHS
    )
}
fn sample_text<T: Copy>(sample: Sample<T>, value: impl FnOnce(T) -> String) -> String {
    match sample {
        Sample::Unavailable => "not measured".into(),
        Sample::Missing => "not present".into(),
        Sample::Fault(fault) => fault_name(fault).into(),
        Sample::Fresh {
            value: v,
            measured_at_ms,
        } => format!(
            "{} (fresh, measured at {} s)",
            value(v),
            measured_at_ms / MILLISECONDS_PER_SECOND
        ),
        Sample::Stale {
            value: v,
            measured_at_ms,
        } => format!(
            "{} (stale, measured at {} s)",
            value(v),
            measured_at_ms / MILLISECONDS_PER_SECOND
        ),
    }
}
fn screen_name(screen: ScreenId) -> &'static str {
    screen.name()
}
fn radio_name(status: RadioStatus) -> &'static str {
    match status {
        RadioStatus::Off => "off",
        RadioStatus::Disconnected => "disconnected",
        RadioStatus::Connected => "connected",
    }
}
fn fault_name(fault: Fault) -> &'static str {
    match fault {
        Fault::SensorCrc => "sensor checksum error",
        Fault::Bus => "bus error",
        Fault::PanelTimeout => "panel timeout",
        Fault::Storage => "storage error",
        Fault::SettingsStorage => "settings flash error",
        Fault::InvalidRtc => "invalid RTC",
        Fault::InvalidInput => "invalid input",
    }
}

fn sample_input<T>(
    input: &str,
    measured_at_ms: u64,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<Sample<T>, &'static str> {
    match input {
        "missing" => Ok(Sample::Missing),
        "fault" => Ok(Sample::Fault(Fault::Bus)),
        "unknown" => Ok(Sample::Unavailable),
        value => Ok(Sample::Fresh {
            value: parse(value).ok_or("invalid measurement")?,
            measured_at_ms,
        }),
    }
}

pub fn rgba(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|byte| {
            [byte >> NIBBLE_BITS, byte & LOW_NIBBLE_MASK]
                .into_iter()
                .flat_map(|nibble| {
                    let rgb = match nibble {
                        WIRE_BLACK => RGB_BLACK,
                        WIRE_YELLOW => RGB_YELLOW,
                        WIRE_RED => RGB_RED,
                        WIRE_BLUE => RGB_BLUE,
                        WIRE_GREEN => RGB_GREEN,
                        _ => RGB_WHITE,
                    };
                    [rgb[0], rgb[1], rgb[2], ALPHA_OPAQUE]
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_budget_matches_independent_wide_arithmetic_at_boundary_combinations() {
        for bytes in [0, 1, MAX_REPLAY_BYTES - 1, MAX_REPLAY_BYTES, usize::MAX] {
            for events in [0, 1, MAX_EVENTS - 1, MAX_EVENTS, usize::MAX] {
                for length in [0, 1, MAX_COMMAND_BYTES, MAX_COMMAND_BYTES + 1, usize::MAX] {
                    let total = bytes as u128 + u128::from(events != 0) + length as u128;
                    assert_eq!(
                        recording_fits(bytes, events, length),
                        length <= MAX_COMMAND_BYTES
                            && events < MAX_EVENTS
                            && total <= MAX_REPLAY_BYTES as u128
                    );
                }
            }
        }
    }
}
