use device_core::{
    app::{App, RefreshState},
    settings::{recover, Settings, SLOT_BYTES},
    telemetry::Fault,
};

const ERASED_FLASH_BYTE: u8 = 0xff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Radios {
    pub wifi: bool,
    pub ble: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepTicket {
    pub sequence: u32,
    pub seconds: u32,
}
impl SleepTicket {
    pub fn current(self, app: App) -> bool {
        app.refresh == RefreshState::Idle
            && !app.pending
            && !app.storage_busy
            && !app.measurement_busy
            && !app.sleep_requested
            && app.sequence == self.sequence
            && app.refresh_seconds == self.seconds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepConditions {
    pub app: App,
    pub ticket: SleepTicket,
    pub buttons_armed: bool,
    pub outputs_quiet: bool,
    pub radios_quiet: bool,
    pub bus_empty: bool,
    pub sleep_pending: bool,
    pub events_empty: bool,
}
impl SleepConditions {
    pub fn permitted(self) -> bool {
        self.ticket.current(self.app)
            && self.buttons_armed
            && self.outputs_quiet
            && self.radios_quiet
            && self.bus_empty
            && !self.sleep_pending
            && self.events_empty
    }
}

pub fn display_is_current(app: App, id: u32) -> bool {
    matches!(app.refresh, RefreshState::Updating { id: active, .. } if active == id)
}

pub fn panel_fault(error: t133a01::Error) -> Fault {
    match error {
        t133a01::Error::BusyTimeout => Fault::PanelTimeout,
        t133a01::Error::Transport
        | t133a01::Error::InvalidFrame
        | t133a01::Error::NotInitialized => Fault::Bus,
    }
}

pub fn loaded_settings(
    first: &[u8; SLOT_BYTES],
    second: &[u8; SLOT_BYTES],
) -> (Settings, Option<Fault>) {
    match recover(first, second) {
        Some((settings, _, _)) => (settings, None),
        None if first
            .iter()
            .chain(second)
            .all(|byte| *byte == ERASED_FLASH_BYTE) =>
        {
            (Settings::default(), None)
        }
        None => (Settings::default(), Some(Fault::SettingsStorage)),
    }
}
