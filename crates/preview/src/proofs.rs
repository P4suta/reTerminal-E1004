use super::*;

#[kani::proof]
#[kani::unwind(21)]
fn native_clock_normalization_preserves_bytes_and_supplies_only_zero_seconds() {
    const INPUT_BYTES: usize = 20;
    const MINUTE_BYTES: usize = 16;
    const SECOND_BYTES: usize = 19;
    let input: [u8; INPUT_BYTES] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= INPUT_BYTES);
    let result = measurements::normalize_clock_bytes(&input[..length]);
    assert_eq!(
        result.is_some(),
        length == MINUTE_BYTES || length == SECOND_BYTES
    );
    if let Some(bytes) = result {
        if length == MINUTE_BYTES {
            assert_eq!(&bytes[..MINUTE_BYTES], &input[..MINUTE_BYTES]);
            assert_eq!(&bytes[MINUTE_BYTES..], b":00");
        } else {
            assert_eq!(bytes, input[..SECOND_BYTES]);
        }
    }
    kani::cover!(length == MINUTE_BYTES);
    kani::cover!(length == SECOND_BYTES);
    kani::cover!(result.is_none());
}

#[kani::proof]
#[kani::unwind(2)]
fn recording_budget_matches_wide_arithmetic_at_every_admitted_boundary() {
    let bytes: usize = kani::any();
    let events: usize = kani::any();
    let length: usize = kani::any();
    let total = bytes as u128 + u128::from(events != 0) + length as u128;
    assert_eq!(
        recording_fits(bytes, events, length),
        length <= MAX_COMMAND_BYTES && events < MAX_EVENTS && total <= MAX_REPLAY_BYTES as u128
    );
    kani::cover!(recording_fits(bytes, events, length) && total == MAX_REPLAY_BYTES as u128);
    kani::cover!(!recording_fits(bytes, events, length) && bytes == usize::MAX);
}

#[kani::proof]
#[kani::unwind(5)]
fn a_cancelled_preview_update_cannot_be_presented_by_mode_switch_or_late_success() {
    let mut preview = Preview::default();
    let rotation = match kani::any::<u8>() % 4 {
        0 => Rotation::Portrait,
        1 => Rotation::Landscape,
        2 => Rotation::InvertedPortrait,
        _ => Rotation::InvertedLandscape,
    };
    let shown = App {
        rotation,
        ..App::default()
    };
    preview.presentation = Presentation::new(shown);
    preview.immediate = false;
    let before = preview.shown();
    let id: u32 = kani::any();
    preview.app = App {
        rotation,
        sequence: id,
        screen: ScreenId::Diagnostics,
        refresh: RefreshState::Updating { id, started_ms: 0 },
        ..App::default()
    };
    preview.presentation.start(id, preview.app);
    assert_eq!(preview.event_step(AppEvent::Tick(DISPLAY_TIMEOUT_MS)), None);
    assert_eq!(preview.presentation.active_id(), None);
    preview.immediate = true;
    preview.complete_active();
    assert_eq!(
        preview.event_step(AppEvent::DisplayFinished {
            id: kani::any(),
            result: Ok(())
        }),
        None
    );
    assert_eq!(preview.shown(), before);
    assert_eq!(
        preview.app.refresh,
        RefreshState::Failed(Fault::PanelTimeout)
    );
    kani::cover!(id == u32::MAX);
}
