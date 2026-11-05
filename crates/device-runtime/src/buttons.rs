use crate::{
    measurements::IoFailure,
    state::{LogRecord, System},
};
use device_core::app::{AppEvent, Button};
use embassy_time::Timer;

const BUTTON_COUNT: usize = 3;
const BUTTON_POLL_MS: u64 = 20;

pub trait ButtonsIo {
    fn is_high(&mut self, index: usize) -> bool;
    fn wake(&mut self, index: usize, enabled: bool) -> Result<(), IoFailure>;
}

pub async fn buttons_loop(system: &System, mut io: impl ButtonsIo) {
    let mut debounce = [e1004_bsp::input::Debouncer::default(); BUTTON_COUNT];
    loop {
        if let Some(arm) = system.button_sleep.try_take() {
            let mut can_arm = arm && (0..BUTTON_COUNT).all(|index| io.is_high(index));
            for index in 0..BUTTON_COUNT {
                if io.wake(index, can_arm).is_err() {
                    can_arm = false;
                    system.record(LogRecord::Event("button_wake_configuration_failed"));
                }
            }
            if !can_arm {
                for index in 0..BUTTON_COUNT {
                    if io.wake(index, false).is_err() {
                        system.record(LogRecord::Event("button_wake_cleanup_failed"));
                    }
                }
            }
            if arm {
                system.button_armed.signal(can_arm);
            }
        }
        for (index, button) in [Button::Previous, Button::Refresh, Button::Next]
            .into_iter()
            .enumerate()
        {
            if debounce[index].update(!io.is_high(index)) {
                system.events.send(AppEvent::Button(button)).await;
            }
        }
        Timer::after_millis(BUTTON_POLL_MS).await;
    }
}
