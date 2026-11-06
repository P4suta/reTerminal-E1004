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
