const STABLE_POLLS: u8 = 3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Debouncer {
    stable_pressed: bool,
    count: u8,
}

impl Debouncer {
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
