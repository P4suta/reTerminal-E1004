use super::*;

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
    preview.event(AppEvent::Configure(device_core::settings::Settings {
        rotation: kani::any::<u8>() % 4,
        ..device_core::settings::Settings::default()
    }));
    assert_eq!(preview.shown().0.rotation, preview.shown().1);
    preview.immediate = false;
    let before = preview.shown();
    preview.event(AppEvent::Button(Button::Next));
    let id = preview.app.sequence;
    preview.event(AppEvent::Tick(DISPLAY_TIMEOUT_MS));
    preview.immediate = true;
    preview.complete_active();
    preview.event(AppEvent::DisplayFinished { id, result: Ok(()) });
    assert_eq!(preview.shown(), before);
    assert_eq!(
        preview.app.refresh,
        RefreshState::Failed(Fault::PanelTimeout)
    );
}
