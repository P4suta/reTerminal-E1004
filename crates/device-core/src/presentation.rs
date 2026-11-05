use crate::telemetry::Fault;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presentation<T> {
    shown: T,
    active: Option<(u32, T)>,
}

impl<T: Copy> Presentation<T> {
    pub const fn new(initial: T) -> Self {
        Self {
            shown: initial,
            active: None,
        }
    }

    pub const fn shown(&self) -> T {
        self.shown
    }

    pub fn active_id(&self) -> Option<u32> {
        self.active.map(|(id, _)| id)
    }

    pub fn start(&mut self, id: u32, snapshot: T) {
        self.active = Some((id, snapshot));
    }

    pub fn finish(&mut self, id: u32, result: Result<(), Fault>) {
        if let Some((active, snapshot)) = self.active {
            if id == active {
                if result.is_ok() {
                    self.shown = snapshot;
                }
                self.active = None;
            }
        }
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }
}
