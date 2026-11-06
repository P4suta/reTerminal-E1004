use crate::{
    geometry::Rotation,
    settings::{ResumePolicy, Settings, DEFAULT_REFRESH_SECONDS},
    telemetry::{Fault, RadioStatus, StorageStatus, TelemetrySnapshot, WakeReason},
};

pub const REFRESH_INTERVAL_MS: u64 = 900_000;
pub const DISPLAY_TIMEOUT_MS: u64 = 120_000;
pub const MILLISECONDS_PER_SECOND: u64 = 1000;
const MAX_ACTIONS: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenId {
    #[default]
    Status,
    Diagnostics,
    Drawing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Previous,
    Refresh,
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshState {
    Idle,
    Updating { id: u32, started_ms: u64 },
    Failed(Fault),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppEvent {
    MeasurementBusy(bool),
    Configure(Settings),
    Wake(WakeReason),
    Button(Button),
    Tick(u64),
    Telemetry(TelemetrySnapshot),
    DisplayFinished { id: u32, result: Result<(), Fault> },
    PanelFailed { id: u32, fault: Fault },
    StorageBusy(bool),
    StorageChanged(StorageStatus),
    SettingsFailed,
    RadioChanged { wifi: RadioStatus, ble: RadioStatus },
    RequestSleep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Measure,
    Display { id: u32 },
    StopPanel,
    Sleep { seconds: u32 },
    Log(Fault),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    pub wake: WakeReason,
    pub storage: StorageStatus,
    pub wifi: RadioStatus,
    pub ble: RadioStatus,
    pub heap_bytes: u32,
    pub psram_bytes: u32,
    pub last_update_ms: u64,
    pub last_error: Option<Fault>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            wake: WakeReason::PowerOn,
            storage: StorageStatus::Absent,
            wifi: RadioStatus::Off,
            ble: RadioStatus::Off,
            heap_bytes: 0,
            psram_bytes: 0,
            last_update_ms: 0,
            last_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct App {
    pub measurement_busy: bool,
    pub rotation: Rotation,
    pub screen: ScreenId,
    pub telemetry: TelemetrySnapshot,
    pub diagnostics: Diagnostics,
    pub refresh: RefreshState,
    pub pending: bool,
    pub storage_busy: bool,
    pub sleep_requested: bool,
    pub now_ms: u64,
    pub sequence: u32,
    pub next_refresh_ms: u64,
    pub refresh_seconds: u32,
    pub resume: ResumePolicy,
    pub persisted_settings: Settings,
}

impl Default for App {
    fn default() -> Self {
        Self {
            measurement_busy: false,
            rotation: Rotation::Portrait,
            screen: ScreenId::Status,
            telemetry: TelemetrySnapshot::default(),
            diagnostics: Diagnostics::default(),
            refresh: RefreshState::Idle,
            pending: false,
            storage_busy: false,
            sleep_requested: false,
            now_ms: 0,
            sequence: 0,
            next_refresh_ms: REFRESH_INTERVAL_MS,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            resume: ResumePolicy::Last,
            persisted_settings: Settings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub state: App,
    pub actions: [Action; MAX_ACTIONS],
}

impl App {
    pub fn settings(self) -> Settings {
        Settings {
            refresh_seconds: self.refresh_seconds,
            rotation: self.rotation as u8,
            resume: self.resume,
            screen: match self.resume {
                ResumePolicy::Last => self.screen,
                ResumePolicy::Status => ScreenId::Status,
            },
        }
    }
    pub fn transition(mut self, event: AppEvent) -> Transition {
        let mut actions = [Action::None; MAX_ACTIONS];
        match event {
            AppEvent::MeasurementBusy(busy) => self.measurement_busy = busy,
            AppEvent::Configure(settings) => {
                if !settings.valid() {
                    return Transition {
                        state: self,
                        actions,
                    };
                }
                let changed = self.refresh_seconds != settings.refresh_seconds
                    || self.rotation as u8 != settings.rotation;
                self.persisted_settings = settings;
                self.resume = settings.resume;
                self.refresh_seconds = settings.refresh_seconds;
                self.rotation = match settings.rotation {
                    0 => Rotation::Portrait,
                    1 => Rotation::Landscape,
                    2 => Rotation::InvertedPortrait,
                    _ => Rotation::InvertedLandscape,
                };
                if changed {
                    self.next_refresh_ms = self.now_ms.saturating_add(
                        u64::from(settings.refresh_seconds) * MILLISECONDS_PER_SECOND,
                    );
                    self.pending = true;
                }
            }
            AppEvent::Wake(reason) => {
                self.diagnostics.wake = reason;
                self.sleep_requested = false;
                self.pending = true;
                actions[0] = Action::Measure;
            }
            AppEvent::Button(button) => {
                self.screen = match (button, self.screen) {
                    (Button::Refresh, screen) => screen,
                    (Button::Next, ScreenId::Status) | (Button::Previous, ScreenId::Drawing) => {
                        ScreenId::Diagnostics
                    }
                    (Button::Next, ScreenId::Diagnostics)
                    | (Button::Previous, ScreenId::Status) => ScreenId::Drawing,
                    (Button::Next, ScreenId::Drawing)
                    | (Button::Previous, ScreenId::Diagnostics) => ScreenId::Status,
                };
                self.pending = true;
                self.sleep_requested = false;
                if button == Button::Refresh {
                    actions[0] = Action::Measure;
                }
                if matches!(self.refresh, RefreshState::Failed(_)) {
                    self.refresh = RefreshState::Idle;
                }
            }
            AppEvent::Tick(now_ms) => {
                self.now_ms = self.now_ms.max(now_ms);
                if let RefreshState::Updating { started_ms, .. } = self.refresh {
                    if self.now_ms.saturating_sub(started_ms) >= DISPLAY_TIMEOUT_MS {
                        self.refresh = RefreshState::Failed(Fault::PanelTimeout);
                        self.diagnostics.last_error = Some(Fault::PanelTimeout);
                        return Transition {
                            state: self,
                            actions: [
                                Action::StopPanel,
                                Action::Log(Fault::PanelTimeout),
                                Action::None,
                            ],
                        };
                    }
                }
                if self.now_ms >= self.next_refresh_ms {
                    if matches!(self.refresh, RefreshState::Failed(_)) {
                        self.refresh = RefreshState::Idle;
                    }
                    self.next_refresh_ms = self
                        .now_ms
                        .saturating_add(u64::from(self.refresh_seconds) * MILLISECONDS_PER_SECOND);
                    self.pending = true;
                    actions[0] = Action::Measure;
                }
                self.telemetry.clock = self.telemetry.clock.at(self.now_ms, REFRESH_INTERVAL_MS);
                self.telemetry.temperature = self
                    .telemetry
                    .temperature
                    .at(self.now_ms, REFRESH_INTERVAL_MS);
                self.telemetry.humidity =
                    self.telemetry.humidity.at(self.now_ms, REFRESH_INTERVAL_MS);
                self.telemetry.battery =
                    self.telemetry.battery.at(self.now_ms, REFRESH_INTERVAL_MS);
            }
            AppEvent::Telemetry(telemetry) => {
                self.telemetry = telemetry;
                self.measurement_busy = false;
                self.pending = true;
            }
            AppEvent::PanelFailed { id, fault } => {
                if id == self.sequence {
                    self.refresh = RefreshState::Failed(fault);
                    self.diagnostics.last_error = Some(fault);
                    self.sleep_requested = false;
                    return Transition {
                        state: self,
                        actions: [Action::StopPanel, Action::Log(fault), Action::None],
                    };
                }
            }
            AppEvent::DisplayFinished { id, result } => {
                let RefreshState::Updating {
                    id: active,
                    started_ms,
                } = self.refresh
                else {
                    return Transition {
                        state: self,
                        actions,
                    };
                };
                if active != id {
                    return Transition {
                        state: self,
                        actions,
                    };
                }
                match result {
                    Ok(()) => {
                        self.refresh = RefreshState::Idle;
                        self.diagnostics.last_update_ms = self.now_ms.saturating_sub(started_ms);
                    }
                    Err(fault) => {
                        self.refresh = RefreshState::Failed(fault);
                        self.diagnostics.last_error = Some(fault);
                        return Transition {
                            state: self,
                            actions: [Action::StopPanel, Action::Log(fault), Action::None],
                        };
                    }
                }
            }
            AppEvent::StorageBusy(busy) => self.storage_busy = busy,
            AppEvent::SettingsFailed => {
                self.diagnostics.last_error = Some(Fault::SettingsStorage);
                if self.screen == ScreenId::Diagnostics {
                    self.pending = true;
                }
                actions[0] = Action::Log(Fault::SettingsStorage);
            }
            AppEvent::StorageChanged(status) => {
                let changed = self.diagnostics.storage != status
                    || (status == StorageStatus::Fault
                        && self.diagnostics.last_error != Some(Fault::Storage));
                if status == StorageStatus::Fault {
                    self.diagnostics.last_error = Some(Fault::Storage);
                    actions[0] = Action::Log(Fault::Storage);
                }
                if self.screen == ScreenId::Diagnostics && changed {
                    self.pending = true;
                }
                self.diagnostics.storage = status;
            }
            AppEvent::RadioChanged { wifi, ble } => {
                if self.screen == ScreenId::Diagnostics
                    && (self.diagnostics.wifi != wifi || self.diagnostics.ble != ble)
                {
                    self.pending = true;
                }
                self.diagnostics.wifi = wifi;
                self.diagnostics.ble = ble;
            }
            AppEvent::RequestSleep => self.sleep_requested = true,
        }
        if actions.contains(&Action::Measure) {
            if self.measurement_busy {
                actions[0] = Action::None;
            } else {
                self.measurement_busy = true;
            }
        }
        if self.pending && !self.measurement_busy && matches!(self.refresh, RefreshState::Idle) {
            self.sequence = self.sequence.wrapping_add(1);
            self.refresh = RefreshState::Updating {
                id: self.sequence,
                started_ms: self.now_ms,
            };
            self.pending = false;
            actions[1] = Action::Display { id: self.sequence };
        }
        if self.sleep_requested
            && !self.measurement_busy
            && !self.storage_busy
            && !self.pending
            && self.refresh == RefreshState::Idle
        {
            self.sleep_requested = false;
            actions[2] = Action::Sleep {
                seconds: self.refresh_seconds,
            };
        }
        Transition {
            state: self,
            actions,
        }
    }
}
