use crate::policy::{Radios, SleepTicket};
use core::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
};
use device_core::{
    app::App,
    settings::{Configuration, ConfigurationId, ConfigurationRequest, Settings},
    telemetry::*,
};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    channel::Channel,
    signal::Signal,
};

pub const WIFI_ACTIVE: u8 = 1;
pub const BLE_ACTIVE: u8 = 2;
pub const EVENT_QUEUE: usize = 32;
pub const BUS_QUEUE: usize = 4;
pub const LOG_QUEUE: usize = 16;
pub const WIFI_QUEUE: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaRequest {
    pub id: u32,
    pub command: e1004_bsp::media::MediaCommand,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaReply {
    pub id: u32,
    pub result: Result<usize, e1004_bsp::media::MediaError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "The single bus owner receives owned snapshots without heap allocation."
)]
pub enum BusRequest {
    Display {
        app: App,
        id: u32,
    },
    Sleep(SleepTicket),
    Save(Settings),
    Configure {
        id: ConfigurationId,
        settings: Settings,
    },
    Diagnose,
    Media(MediaRequest),
}

#[derive(Debug)]
pub enum WifiCommand {
    Scan,
    Connect {
        ssid: heapless::String<32>,
        password: heapless::String<64>,
    },
    Disconnect,
    Quiesce,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogRecord {
    Fault(Fault),
    Event(&'static str),
    Scan(u8),
    Network {
        ssid: heapless::String<32>,
        signal_dbm: i8,
    },
}

pub struct System {
    pub events: Channel<CriticalSectionRawMutex, device_core::app::AppEvent, EVENT_QUEUE>,
    pub bus: Channel<CriticalSectionRawMutex, BusRequest, BUS_QUEUE>,
    pub sleep_request: Signal<CriticalSectionRawMutex, SleepTicket>,
    pub measure: Signal<CriticalSectionRawMutex, ()>,
    pub rtc_set: Signal<CriticalSectionRawMutex, DateTime>,
    pub media_reply: Signal<CriticalSectionRawMutex, MediaReply>,
    pub button_sleep: Signal<CriticalSectionRawMutex, bool>,
    pub button_armed: Signal<CriticalSectionRawMutex, bool>,
    pub output_sleep: Signal<CriticalSectionRawMutex, bool>,
    pub output_quiet: Signal<CriticalSectionRawMutex, ()>,
    pub stop_panel: Signal<CriticalSectionRawMutex, ()>,
    pub logs: Channel<CriticalSectionRawMutex, LogRecord, LOG_QUEUE>,
    pub lost_logs: AtomicU32,
    pub sleeping: AtomicBool,
    pub wifi_command: Channel<CriticalSectionRawMutex, WifiCommand, WIFI_QUEUE>,
    pub ble_enabled: Signal<CriticalSectionRawMutex, bool>,
    pub battery: Signal<CriticalSectionRawMutex, u8>,
    pub radio_changed: Signal<CriticalSectionRawMutex, ()>,
    pub radio_active: AtomicU8,
    pub capabilities: Radios,
    pub extension: Mutex<CriticalSectionRawMutex, Cell<Sample<u16>>>,
    snapshot: Mutex<CriticalSectionRawMutex, Cell<Option<App>>>,
    requested_configuration: Mutex<CriticalSectionRawMutex, Cell<Option<ConfigurationRequest>>>,
    configuration_sequence: Mutex<CriticalSectionRawMutex, Cell<Option<u32>>>,
    media_sequence: Mutex<CriticalSectionRawMutex, Cell<Option<u32>>>,
    display: Mutex<
        CriticalSectionRawMutex,
        RefCell<device_core::presentation::Presentation<Option<App>>>,
    >,
    radios: Mutex<CriticalSectionRawMutex, Cell<(RadioStatus, RadioStatus)>>,
}
impl System {
    pub const fn new(capabilities: Radios) -> Self {
        Self {
            events: Channel::new(),
            bus: Channel::new(),
            sleep_request: Signal::new(),
            measure: Signal::new(),
            rtc_set: Signal::new(),
            media_reply: Signal::new(),
            button_sleep: Signal::new(),
            button_armed: Signal::new(),
            output_sleep: Signal::new(),
            output_quiet: Signal::new(),
            stop_panel: Signal::new(),
            logs: Channel::new(),
            lost_logs: AtomicU32::new(0),
            sleeping: AtomicBool::new(false),
            wifi_command: Channel::new(),
            ble_enabled: Signal::new(),
            battery: Signal::new(),
            radio_changed: Signal::new(),
            radio_active: AtomicU8::new(0),
            capabilities,
            extension: Mutex::new(Cell::new(Sample::Unavailable)),
            snapshot: Mutex::new(Cell::new(None)),
            requested_configuration: Mutex::new(Cell::new(None)),
            configuration_sequence: Mutex::new(Cell::new(Some(0))),
            media_sequence: Mutex::new(Cell::new(Some(0))),
            display: Mutex::new(RefCell::new(device_core::presentation::Presentation::new(
                None,
            ))),
            radios: Mutex::new(Cell::new((RadioStatus::Off, RadioStatus::Off))),
        }
    }
    pub fn snapshot(&self) -> Option<App> {
        self.snapshot.lock(Cell::get)
    }
    pub fn publish(&self, app: App) {
        self.snapshot.lock(|state| state.set(Some(app)));
    }
    pub fn configuration_settings(&self) -> Settings {
        let app = self.snapshot().unwrap_or_default();
        match self.requested_configuration.lock(Cell::get) {
            Some(request) => app
                .settings()
                .with_configuration(request.value(), app.screen),
            None => app.settings(),
        }
    }
    pub fn reserve_configuration(&self, value: Configuration) -> Option<ConfigurationRequest> {
        self.configuration_sequence.lock(|counter| {
            let sequence = counter.get()?;
            counter.set(sequence.checked_add(1));
            Some(ConfigurationRequest::new(
                ConfigurationId::new(sequence),
                value,
            ))
        })
    }
    pub fn request_configuration(&self, request: ConfigurationRequest) {
        self.requested_configuration
            .lock(|requested| requested.set(Some(request)));
    }
    pub fn reserve_media(&self, command: e1004_bsp::media::MediaCommand) -> Option<MediaRequest> {
        self.media_sequence.lock(|counter| {
            let id = counter.get()?;
            counter.set(id.checked_add(1));
            Some(MediaRequest { id, command })
        })
    }
    pub fn acknowledge_configuration(&self, id: ConfigurationId) {
        self.requested_configuration.lock(|requested| {
            requested.set(requested.get().and_then(|desired| desired.acknowledge(id)));
        });
    }
    pub fn presented(&self) -> Option<App> {
        self.display.lock(|display| display.borrow().shown())
    }
    pub fn start_display(&self, id: u32, app: App) {
        self.display
            .lock(|display| display.borrow_mut().start(id, Some(app)));
    }
    pub fn finish_display(&self, id: u32, result: Result<(), Fault>) {
        self.display
            .lock(|display| display.borrow_mut().finish(id, result));
    }
    pub fn cancel_display(&self) {
        self.display.lock(|display| display.borrow_mut().cancel());
    }
    pub fn record(&self, record: LogRecord) {
        if self.logs.try_send(record).is_err() {
            self.lost_logs.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn active(&self, radio: u8, active: bool) {
        if active {
            self.radio_active.fetch_or(radio, Ordering::AcqRel);
        } else {
            self.radio_active.fetch_and(!radio, Ordering::AcqRel);
        }
        self.radio_changed.signal(());
    }
    pub fn radios_quiet(&self) -> bool {
        self.radio_active.load(Ordering::Acquire) == 0
            && self.wifi_command.is_empty()
            && !self.ble_enabled.signaled()
    }
    pub async fn radio_status(&self, wifi: Option<RadioStatus>, ble: Option<RadioStatus>) {
        let event = self.radios.lock(|state| {
            let old = state.get();
            let value = (wifi.unwrap_or(old.0), ble.unwrap_or(old.1));
            state.set(value);
            device_core::app::AppEvent::RadioChanged {
                wifi: value.0,
                ble: value.1,
            }
        });
        self.events.send(event).await;
    }
}
