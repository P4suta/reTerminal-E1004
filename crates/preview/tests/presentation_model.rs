use device_core::app::{RefreshState, ScreenId};
use device_preview::Preview;

#[derive(Clone, Copy, Debug)]
enum Input {
    Previous,
    Next,
    Refresh,
    Hold,
    Immediate,
    Complete,
    Timeout,
    Elapse,
}
const INPUTS: [Input; 8] = [
    Input::Previous,
    Input::Next,
    Input::Refresh,
    Input::Hold,
    Input::Immediate,
    Input::Complete,
    Input::Timeout,
    Input::Elapse,
];
const DEPTH: usize = 6;
const SCREEN_COUNT: u8 = 7;
const TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy, Default)]
struct Model {
    desired: u8,
    shown: u8,
    active: Option<u8>,
    pending: bool,
    failed: bool,
    hold: bool,
    clock: u64,
    started: u64,
}
impl Model {
    fn step(&mut self, input: Input) {
        match input {
            Input::Previous | Input::Next | Input::Refresh => {
                self.desired = (self.desired
                    + match input {
                        Input::Previous => SCREEN_COUNT - 1,
                        Input::Next => 1,
                        _ => 0,
                    })
                    % SCREEN_COUNT;
                self.pending = true;
                self.failed = false;
            }
            Input::Hold => self.hold = true,
            Input::Immediate => self.hold = false,
            Input::Complete => {
                if let Some(snapshot) = self.active.take() {
                    self.shown = snapshot;
                }
            }
            Input::Timeout => {
                if self.active.take().is_some() {
                    self.failed = true;
                }
            }
            Input::Elapse => {
                self.clock += TIMEOUT_MS;
                if self.active.is_some() && self.clock - self.started >= TIMEOUT_MS {
                    self.active = None;
                    self.failed = true;
                }
            }
        }
        if self.active.is_none() && self.pending && !self.failed {
            self.active = Some(self.desired);
            self.started = self.clock;
            self.pending = false;
        }
        if !self.hold {
            if let Some(snapshot) = self.active.take() {
                self.shown = snapshot;
                if self.pending {
                    self.shown = self.desired;
                    self.pending = false;
                }
            }
        }
    }
}

fn screen(index: u8) -> ScreenId {
    match index {
        0 => ScreenId::Status,
        1 => ScreenId::Calendar,
        2 => ScreenId::Checklist,
        3 => ScreenId::Notes,
        4 => ScreenId::Diagnostics,
        5 => ScreenId::Drawing,
        6 => ScreenId::Gallery,
        _ => unreachable!(),
    }
}

fn explore(preview: &Preview, model: Model, path: &mut Vec<Input>, leaves: &mut usize) {
    if path.len() == DEPTH {
        *leaves += 1;
        return;
    }
    for input in INPUTS {
        let mut actual = preview.clone();
        let mut expected = model;
        expected.step(input);
        path.push(input);
        let command = match input {
            Input::Previous => "previous".to_owned(),
            Input::Next => "next".to_owned(),
            Input::Refresh => "refresh".to_owned(),
            Input::Hold => "wait".to_owned(),
            Input::Immediate => "immediate".to_owned(),
            Input::Complete => "complete".to_owned(),
            Input::Timeout => "timeout".to_owned(),
            Input::Elapse => format!("tick {}", expected.clock),
        };
        actual.command(&command).unwrap();
        assert_eq!(actual.app().screen, screen(expected.desired), "{path:?}");
        assert_eq!(actual.shown().0.screen, screen(expected.shown), "{path:?}");
        assert_eq!(
            matches!(actual.app().refresh, RefreshState::Updating { .. }),
            expected.active.is_some(),
            "{path:?}"
        );
        assert_eq!(
            matches!(actual.app().refresh, RefreshState::Failed(_)),
            expected.failed,
            "{path:?}"
        );
        assert_eq!(actual.app().pending, expected.pending, "{path:?}");
        assert_eq!(actual.immediate(), !expected.hold, "{path:?}");
        explore(&actual, expected, path, leaves);
        path.pop();
    }
}

#[test]
fn every_six_event_completion_sequence_matches_an_independent_display_model() {
    let mut leaves = 0;
    explore(
        &Preview::default(),
        Model::default(),
        &mut Vec::new(),
        &mut leaves,
    );
    assert_eq!(leaves, INPUTS.len().pow(DEPTH as u32));
}
