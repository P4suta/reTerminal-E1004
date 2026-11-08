use crate::{
    measurements::IoFailure,
    state::{LogRecord, System},
};
use device_core::app::{AppEvent, Button};
use embassy_time::{Instant, Timer};

const BUTTON_COUNT: usize = 3;
const BUTTON_POLL_MS: u64 = 20;

pub trait ButtonsIo {
    fn is_high(&mut self, index: usize) -> bool;
    fn wake(&mut self, index: usize, enabled: bool) -> Result<(), IoFailure>;
}

pub async fn buttons_loop(system: &System, mut io: impl ButtonsIo) {
    let mut gestures = [e1004_bsp::input::Gesture::default(); BUTTON_COUNT];
    loop {
        if let Some(arm) = system.button_sleep.try_take() {
            let mut can_arm = arm
                && (0..BUTTON_COUNT).all(|index| io.is_high(index))
                && gestures.iter().all(e1004_bsp::input::Gesture::is_idle);
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
            if let Some(gesture) =
                gestures[index].update(!io.is_high(index), Instant::now().as_millis())
            {
                let event = match gesture {
                    e1004_bsp::input::GestureEvent::Tap => AppEvent::Button(button),
                    e1004_bsp::input::GestureEvent::Hold => AppEvent::Hold(button),
                };
                system.events.send(event).await;
            }
        }
        Timer::after_millis(BUTTON_POLL_MS).await;
    }
}
