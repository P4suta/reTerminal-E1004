const STABLE_POLLS: u8 = 3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Debouncer {
    stable_pressed: bool,
    count: u8,
}

impl Debouncer {
    pub const fn pressed(&self) -> bool {
        self.stable_pressed
    }
    pub fn update(&mut self, pressed: bool) -> bool {
        if pressed == self.stable_pressed {
            self.count = 0;
            return false;
        }
        self.count += 1;
        if self.count == STABLE_POLLS {
            self.count = 0;
            self.stable_pressed = pressed;
            return pressed;
        }
        false
    }
}

pub const HOLD_MS: u64 = 800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureEvent {
    Tap,
    Hold,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Gesture {
    debounce: Debouncer,
    started_ms: Option<u64>,
    held: bool,
}

impl Gesture {
    pub const fn is_idle(&self) -> bool {
        self.started_ms.is_none()
    }

    pub fn update(&mut self, pressed: bool, now_ms: u64) -> Option<GestureEvent> {
        let was_pressed = self.debounce.pressed();
        self.debounce.update(pressed);
        if self.debounce.pressed() && !was_pressed {
            self.started_ms = Some(now_ms);
            self.held = false;
        }
        if !self.debounce.pressed() && was_pressed {
            self.started_ms = None;
            return if self.held {
                None
            } else {
                Some(GestureEvent::Tap)
            };
        }
        if !self.held
            && self
                .started_ms
                .is_some_and(|started| now_ms.saturating_sub(started) >= HOLD_MS)
        {
            self.held = true;
            return Some(GestureEvent::Hold);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounce_and_holds_emit_one_press_after_three_consecutive_polls() {
        let mut input = Debouncer::default();
        for value in [true, false, true, false, true, true] {
            assert!(!input.update(value));
        }
        assert!(input.update(true));
        for _ in 0..100 {
            assert!(!input.update(true));
        }
        for _ in 0..3 {
            assert!(!input.update(false));
        }
        assert!(!input.update(true));
        assert!(!input.update(true));
        assert!(input.update(true));
    }
}
