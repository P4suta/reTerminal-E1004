use crate::{
    calendar::Month,
    daily::{Daily, DailyEdit, TaskId},
    geometry::Rotation,
    media::{Gallery, GalleryEvent, SlideInterval},
    settings::{ConfigurationId, ResumePolicy, Settings, DEFAULT_REFRESH_SECONDS},
    telemetry::{Fault, RadioStatus, Sample, StorageStatus, TelemetrySnapshot, WakeReason},
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
    Calendar,
    Checklist,
    Notes,
    Gallery,
}

impl ScreenId {
    pub const ALL: [Self; 7] = [
        Self::Status,
        Self::Calendar,
        Self::Checklist,
        Self::Notes,
        Self::Diagnostics,
        Self::Drawing,
        Self::Gallery,
    ];
    pub const fn code(self) -> u8 {
        match self {
            Self::Status => 0,
            Self::Diagnostics => 1,
            Self::Drawing => 2,
            Self::Calendar => 3,
            Self::Checklist => 4,
            Self::Notes => 5,
            Self::Gallery => 6,
        }
    }
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Status),
            1 => Some(Self::Diagnostics),
            2 => Some(Self::Drawing),
            3 => Some(Self::Calendar),
            4 => Some(Self::Checklist),
            5 => Some(Self::Notes),
            6 => Some(Self::Gallery),
            _ => None,
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "status" => Some(Self::Status),
            "calendar" => Some(Self::Calendar),
            "checklist" => Some(Self::Checklist),
            "notes" => Some(Self::Notes),
            "diagnostics" => Some(Self::Diagnostics),
            "drawing" => Some(Self::Drawing),
            "gallery" => Some(Self::Gallery),
            _ => None,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Status => "Device Status",
            Self::Calendar => "Calendar",
            Self::Checklist => "Checklist",
            Self::Notes => "Notes",
            Self::Diagnostics => "Diagnostics",
            Self::Drawing => "Drawing",
            Self::Gallery => "Gallery",
        }
    }
    pub const fn step(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Status, true) | (Self::Checklist, false) => Self::Calendar,
            (Self::Calendar, true) | (Self::Notes, false) => Self::Checklist,
            (Self::Checklist, true) | (Self::Diagnostics, false) => Self::Notes,
            (Self::Notes, true) | (Self::Drawing, false) => Self::Diagnostics,
            (Self::Diagnostics, true) | (Self::Gallery, false) => Self::Drawing,
            (Self::Drawing, true) | (Self::Status, false) => Self::Gallery,
            (Self::Gallery, true) | (Self::Calendar, false) => Self::Status,
        }
    }
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
    ConfigurationSaved {
        id: ConfigurationId,
        settings: Settings,
    },
    Wake(WakeReason),
    Button(Button),
    Hold(Button),
    SelectScreen(ScreenId),
    EditDaily(DailyEdit),
    Gallery(GalleryEvent),
    ResumeGallery(Settings),
    Tick(u64),
    Telemetry(TelemetrySnapshot),
    DisplayFinished {
        id: u32,
        result: Result<(), Fault>,
    },
    PanelFailed {
        id: u32,
        fault: Fault,
    },
    StorageBusy(bool),
    StorageChanged(StorageStatus),
    SettingsFailed,
    RadioChanged {
        wifi: RadioStatus,
        ble: RadioStatus,
    },
    RequestSleep,
    KeepAwake(u32),
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
    pub daily: Daily,
    pub selected_task: TaskId,
    pub calendar_month: Option<Month>,
    pub gallery: Gallery,
    pub awake_until_ms: u64,
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
            daily: Daily::default(),
            selected_task: TaskId::default(),
            calendar_month: None,
            gallery: Gallery::default(),
            awake_until_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub state: App,
    pub actions: [Action; MAX_ACTIONS],
}

impl App {
    pub fn sleep_seconds(self) -> u32 {
        if self.screen == ScreenId::Gallery {
            if let Some(interval) = self.gallery.settings().slideshow {
                return interval.seconds();
            }
            if self.gallery.view().image.is_some() {
                return 0;
            }
        }
        self.refresh_seconds
    }
    pub fn awake(self) -> bool {
        self.now_ms < self.awake_until_ms
    }
    pub fn settings(self) -> Settings {
        Settings {
            refresh_seconds: self.refresh_seconds,
            rotation: self.rotation as u8,
            resume: self.resume,
            screen: match self.resume {
                ResumePolicy::Last => self.screen,
                ResumePolicy::Status => ScreenId::Status,
            },
            daily: self.daily,
            selected_task: self.selected_task,
            calendar_month: self.calendar_month,
            gallery: self.gallery.persistent_settings(),
        }
    }
    pub fn calendar(self) -> Option<Month> {
        self.calendar_month.or_else(|| match self.telemetry.clock {
            Sample::Fresh { value, .. } | Sample::Stale { value, .. } if value.valid() => {
                Month::new(value.year, value.month)
            }
            _ => None,
        })
    }
    pub fn middle_label(self) -> &'static str {
        match self.screen {
            ScreenId::Calendar => "TODAY",
            ScreenId::Checklist if !self.daily.task(self.selected_task).text.is_empty() => {
                if self.daily.task(self.selected_task).done {
                    "REOPEN"
                } else {
                    "DONE"
                }
            }
            ScreenId::Gallery if !self.gallery.catalog().is_empty() => {
                if self.gallery.settings().slideshow.is_some() {
                    "PAUSE"
                } else {
                    "START"
                }
            }
            ScreenId::Status
            | ScreenId::Checklist
            | ScreenId::Notes
            | ScreenId::Diagnostics
            | ScreenId::Drawing => "REFRESH",
            ScreenId::Gallery => "REFRESH",
        }
    }
    pub fn transition(mut self, event: AppEvent) -> Transition {
        let mut actions = [Action::None; MAX_ACTIONS];
        match event {
            AppEvent::MeasurementBusy(busy) => self.measurement_busy = busy,
            AppEvent::Configure(settings) | AppEvent::ConfigurationSaved { settings, .. } => {
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
                if reason == WakeReason::Timer && self.screen == ScreenId::Gallery {
                    self.gallery = self
                        .gallery
                        .transition(GalleryEvent::WakeTimer, self.now_ms);
                }
                self.diagnostics.wake = reason;
                self.sleep_requested = false;
                self.pending = true;
                actions[0] = Action::Measure;
            }
            AppEvent::Button(button) => {
                let gallery_controls =
                    self.screen == ScreenId::Gallery && !self.gallery.catalog().is_empty();
                let previous_view = self.gallery.view();
                match button {
                    Button::Previous | Button::Next if gallery_controls => {
                        self.gallery = self
                            .gallery
                            .transition(GalleryEvent::Step(button == Button::Next), self.now_ms);
                    }
                    Button::Previous => self.screen = self.screen.step(false),
                    Button::Next => self.screen = self.screen.step(true),
                    Button::Refresh => match self.screen {
                        ScreenId::Gallery if gallery_controls => {
                            let interval = if self.gallery.settings().slideshow.is_some() {
                                None
                            } else {
                                Some(SlideInterval::default())
                            };
                            self.gallery = self
                                .gallery
                                .transition(GalleryEvent::Slideshow(interval), self.now_ms);
                        }
                        ScreenId::Calendar => self.calendar_month = None,
                        ScreenId::Checklist
                            if !self.daily.task(self.selected_task).text.is_empty() =>
                        {
                            self.daily = self.daily.edit(DailyEdit::Toggle(self.selected_task))
                        }
                        ScreenId::Status
                        | ScreenId::Checklist
                        | ScreenId::Notes
                        | ScreenId::Diagnostics
                        | ScreenId::Drawing
                        | ScreenId::Gallery => actions[0] = Action::Measure,
                    },
                }
                if gallery_controls && button == Button::Refresh {
                    self.sleep_requested = true;
                } else {
                    self.pending |= !gallery_controls || previous_view != self.gallery.view();
                    self.sleep_requested = false;
                }
                if matches!(self.refresh, RefreshState::Failed(_)) {
                    self.refresh = RefreshState::Idle;
                }
            }
            AppEvent::SelectScreen(screen) => {
                if self.screen != screen {
                    self.screen = screen;
                    self.pending = true;
                    self.sleep_requested = false;
                }
            }
            AppEvent::EditDaily(edit) => {
                let daily = self.daily.edit(edit);
                if daily != self.daily {
                    self.daily = daily;
                    self.pending = true;
                    self.sleep_requested = false;
                }
            }
            AppEvent::Gallery(event) => {
                if matches!(event, GalleryEvent::Committed(..)) {
                    self.screen = ScreenId::Gallery;
                }
                let previous = self.gallery.view();
                let was_busy = self.gallery.busy();
                self.gallery = self.gallery.transition(event, self.now_ms);
                if self.gallery.busy()
                    && !was_busy
                    && matches!(self.refresh, RefreshState::Updating { .. })
                {
                    self.refresh = RefreshState::Idle;
                    self.pending = true;
                    actions[0] = Action::StopPanel;
                }
                if self.screen == ScreenId::Gallery && self.gallery.view() != previous {
                    self.pending = true;
                    self.sleep_requested = false;
                    if matches!(self.refresh, RefreshState::Failed(_)) {
                        self.refresh = RefreshState::Idle;
                    }
                }
            }
            AppEvent::ResumeGallery(settings) => {
                if let Some(selected) =
                    settings.retained_selection(self.persisted_settings, self.diagnostics.wake)
                {
                    self.gallery = self
                        .gallery
                        .transition(GalleryEvent::Resume(selected), self.now_ms);
                }
            }
            AppEvent::Hold(button) => {
                match (button, self.screen) {
                    (Button::Refresh, _) => {
                        actions[0] = Action::Measure;
                        if matches!(self.refresh, RefreshState::Failed(_)) {
                            self.refresh = RefreshState::Idle;
                        }
                    }
                    (Button::Previous | Button::Next, ScreenId::Calendar) => {
                        let month = self
                            .calendar()
                            .map(|month| month.step(button == Button::Next));
                        if month == self.calendar() {
                            return Transition {
                                state: self,
                                actions,
                            };
                        }
                        self.calendar_month = month;
                    }
                    (Button::Previous | Button::Next, ScreenId::Checklist) => {
                        self.selected_task = self.selected_task.step(button == Button::Next)
                    }
                    (Button::Previous | Button::Next, ScreenId::Gallery) => {
                        self.screen = self.screen.step(button == Button::Next);
                    }
                    (
                        Button::Previous | Button::Next,
                        ScreenId::Status
                        | ScreenId::Notes
                        | ScreenId::Diagnostics
                        | ScreenId::Drawing,
                    ) => {
                        return Transition {
                            state: self,
                            actions,
                        }
                    }
                }
                self.pending = true;
                self.sleep_requested = false;
            }
            AppEvent::Tick(now_ms) => {
                self.now_ms = self.now_ms.max(now_ms);
                if self.screen == ScreenId::Gallery {
                    let previous = self.gallery.view();
                    self.gallery = self.gallery.transition(GalleryEvent::Tick, self.now_ms);
                    if self.gallery.view() != previous {
                        self.pending = true;
                        if matches!(self.refresh, RefreshState::Failed(_)) {
                            self.refresh = RefreshState::Idle;
                        }
                    }
                }
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
                    let refresh_content = self.screen != ScreenId::Gallery
                        || self.gallery.view().image.is_none()
                        || matches!(self.refresh, RefreshState::Failed(_));
                    self.next_refresh_ms = self
                        .now_ms
                        .saturating_add(u64::from(self.refresh_seconds) * MILLISECONDS_PER_SECOND);
                    if refresh_content {
                        if matches!(self.refresh, RefreshState::Failed(_)) {
                            self.refresh = RefreshState::Idle;
                        }
                        self.pending = true;
                        actions[0] = Action::Measure;
                    }
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
            AppEvent::KeepAwake(seconds) => {
                if seconds <= 3600 {
                    self.awake_until_ms = self.now_ms.saturating_add(u64::from(seconds) * 1000);
                }
            }
        }
        if actions.contains(&Action::Measure) {
            if self.measurement_busy {
                actions[0] = Action::None;
            } else {
                self.measurement_busy = true;
            }
        }
        if self.pending
            && !self.measurement_busy
            && !self.gallery.busy()
            && matches!(self.refresh, RefreshState::Idle)
        {
            self.sequence = self.sequence.wrapping_add(1);
            self.refresh = RefreshState::Updating {
                id: self.sequence,
                started_ms: self.now_ms,
            };
            self.pending = false;
            actions[1] = Action::Display { id: self.sequence };
        }
        if self.sleep_requested
            && !self.awake()
            && !self.measurement_busy
            && !self.storage_busy
            && !self.gallery.busy()
            && !self.pending
            && self.refresh == RefreshState::Idle
        {
            self.sleep_requested = false;
            actions[2] = Action::Sleep {
                seconds: self.sleep_seconds(),
            };
        }
        Transition {
            state: self,
            actions,
        }
    }
}
