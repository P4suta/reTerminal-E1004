use device_core::{
    app::{Action, App, AppEvent, Button, RefreshState, ScreenId},
    geometry::Rotation,
    image::{Anchor, Fit},
    media::{
        Catalog, Gallery, GalleryEvent, GallerySettings, ImageId, ImageIndex, ImageRecord,
        SlideInterval, IMAGE_SLOTS,
    },
    settings::{decode, encode, Settings},
    telemetry::WakeReason,
};

#[test]
fn awake_leases_are_bounded_and_slideshow_sleep_uses_its_own_interval() {
    let mut app = App::default().transition(AppEvent::KeepAwake(120)).state;
    let request = app.transition(AppEvent::RequestSleep);
    assert!(!request
        .actions
        .iter()
        .any(|a| matches!(a, Action::Sleep { .. })));
    app = request.state;
    app = app.transition(AppEvent::KeepAwake(3601)).state;
    assert!(!app
        .transition(AppEvent::Tick(119999))
        .actions
        .iter()
        .any(|a| matches!(a, Action::Sleep { .. })));
    assert!(app
        .transition(AppEvent::Tick(120000))
        .actions
        .iter()
        .any(|a| matches!(a, Action::Sleep { .. })));
    app.screen = ScreenId::Gallery;
    app.gallery = app
        .gallery
        .transition(GalleryEvent::Slideshow(SlideInterval::new(60)), 0);
    assert_eq!(app.sleep_seconds(), 60);
}

#[test]
fn a_static_gallery_image_sleeps_until_button_input_without_periodic_redrawing() {
    let mut app = App {
        screen: ScreenId::Gallery,
        ..App::default()
    };
    app.gallery = Gallery::new(GallerySettings::default(), catalog());
    assert_eq!(app.sleep_seconds(), 0);
    let transition = app.transition(AppEvent::Tick(900000));
    assert!(!transition
        .actions
        .iter()
        .any(|action| matches!(action, Action::Measure | Action::Display { .. })));
    assert!(app
        .transition(AppEvent::RequestSleep)
        .actions
        .contains(&Action::Sleep { seconds: 0 }));
}

fn catalog() -> Catalog {
    let mut records = [None; IMAGE_SLOTS];
    for (slot, id) in [1, 3, 8].into_iter().enumerate() {
        records[slot] = Some(ImageRecord::image(
            ImageId::new(id).unwrap(),
            slot as u64 + 1,
            Rotation::Portrait,
            slot as u32,
        ));
    }
    ImageIndex::new(records).catalog()
}

#[test]
fn automatic_slides_preserve_manual_preferences_and_resume_only_matching_deep_sleep_state() {
    let base = Settings {
        screen: ScreenId::Gallery,
        gallery: GallerySettings {
            slideshow: SlideInterval::new(60),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut app = App {
        screen: ScreenId::Gallery,
        persisted_settings: base,
        gallery: Gallery::new(base.gallery, catalog()),
        ..Default::default()
    };
    app = app
        .transition(AppEvent::Gallery(GalleryEvent::Slideshow(
            SlideInterval::new(60),
        )))
        .state;
    app = app.transition(AppEvent::Tick(60000)).state;
    assert_eq!(app.gallery.settings().selected.number(), 3);
    assert_eq!(app.settings(), base);
    let retained = Settings {
        gallery: app.gallery.settings(),
        ..app.settings()
    };
    let startup = App {
        screen: ScreenId::Gallery,
        persisted_settings: base,
        gallery: Gallery::loading(base.gallery),
        ..Default::default()
    };
    let resumed = startup
        .transition(AppEvent::Wake(WakeReason::Timer))
        .state
        .transition(AppEvent::ResumeGallery(retained))
        .state
        .transition(AppEvent::Gallery(GalleryEvent::Ready(catalog())))
        .state;
    assert_eq!(resumed.gallery.settings().selected.number(), 8);
    assert_eq!(resumed.settings(), base);
    let cold = startup
        .transition(AppEvent::Wake(WakeReason::PowerOn))
        .state
        .transition(AppEvent::ResumeGallery(retained))
        .state
        .transition(AppEvent::Gallery(GalleryEvent::Ready(catalog())))
        .state;
    assert_eq!(cold.gallery.settings().selected.number(), 1);
    let mismatched = startup
        .transition(AppEvent::Wake(WakeReason::Timer))
        .state
        .transition(AppEvent::ResumeGallery(Settings {
            rotation: 2,
            ..retained
        }))
        .state
        .transition(AppEvent::Gallery(GalleryEvent::Ready(catalog())))
        .state;
    assert_eq!(mismatched.gallery.settings().selected.number(), 3);
}

#[test]
fn retained_selection_is_admitted_only_during_loading_and_preserves_pending_timer_advance() {
    let settings = GallerySettings {
        slideshow: SlideInterval::new(60),
        ..Default::default()
    };
    let ready = Gallery::new(settings, catalog());
    let receiving = ready.transition(GalleryEvent::Busy(true), 0);
    for gallery in [ready, receiving] {
        assert_eq!(
            gallery.transition(GalleryEvent::Resume(ImageId::new(8).unwrap()), 30000),
            gallery
        );
    }
    let resumed = Gallery::loading(settings)
        .transition(GalleryEvent::WakeTimer, 0)
        .transition(GalleryEvent::Resume(ImageId::new(8).unwrap()), 0)
        .transition(GalleryEvent::Ready(catalog()), 0);
    assert_eq!(resumed.settings().selected.number(), 1);
    assert_eq!(resumed.persistent_settings(), settings);
}

#[test]
fn gallery_selection_skips_empty_slots_and_scheduling_coalesces_elapsed_intervals() {
    let mut gallery = Gallery::new(GallerySettings::default(), catalog());
    assert_eq!(gallery.settings().selected.number(), 1);
    gallery = gallery.transition(GalleryEvent::Step(true), 0);
    assert_eq!(gallery.settings().selected.number(), 3);
    gallery = gallery.transition(GalleryEvent::Step(false), 0);
    assert_eq!(gallery.settings().selected.number(), 1);
    gallery = gallery.transition(GalleryEvent::Step(false), 0);
    assert_eq!(gallery.settings().selected.number(), 8);
    let interval = SlideInterval::new(60).unwrap();
    gallery = gallery.transition(GalleryEvent::Slideshow(Some(interval)), 1000);
    assert_eq!(
        gallery
            .transition(GalleryEvent::Tick, 60_999)
            .settings()
            .selected
            .number(),
        8
    );
    gallery = gallery.transition(GalleryEvent::Tick, 1_000_000);
    assert_eq!(gallery.settings().selected.number(), 1);
    assert_eq!(
        gallery
            .transition(GalleryEvent::Tick, 1_000_000)
            .settings()
            .selected
            .number(),
        1
    );
    assert!(SlideInterval::new(59).is_none());
    assert!(SlideInterval::new(86401).is_none());
}

#[test]
fn loading_and_transfers_hold_sleep_and_do_not_advance_the_visible_image() {
    let settings = GallerySettings {
        slideshow: SlideInterval::new(60),
        ..GallerySettings::default()
    };
    let loading = Gallery::loading(settings).transition(GalleryEvent::WakeTimer, 0);
    let interrupted_load = Gallery::loading(settings)
        .transition(GalleryEvent::Busy(true), 0)
        .transition(GalleryEvent::WakeTimer, 0)
        .transition(GalleryEvent::Ready(catalog()), 0);
    assert_eq!(interrupted_load.settings().selected.number(), 3);
    assert!(Gallery::loading(settings)
        .transition(GalleryEvent::Busy(false), 0)
        .busy());
    let scheduled = Gallery::new(settings, catalog())
        .transition(GalleryEvent::Slideshow(SlideInterval::new(60)), 0)
        .transition(GalleryEvent::Busy(false), 59000)
        .transition(GalleryEvent::Tick, 60000);
    assert_eq!(scheduled.settings().selected.number(), 3);
    assert_eq!(
        Gallery::new(GallerySettings::default(), catalog())
            .transition(GalleryEvent::WakeTimer, 0)
            .settings()
            .selected
            .number(),
        1
    );
    assert!(loading.busy());
    let loaded = loading.transition(GalleryEvent::Ready(catalog()), 0);
    assert!(!loaded.busy());
    assert_eq!(loaded.settings().selected.number(), 3);
    let receiving = loaded.transition(GalleryEvent::Busy(true), 0);
    assert!(receiving.busy());
    assert_eq!(
        receiving.transition(GalleryEvent::Tick, 100_000).view(),
        loaded.view()
    );
    assert!(!receiving.transition(GalleryEvent::Busy(false), 0).busy());
    assert_eq!(
        Gallery::new(GallerySettings::default(), Catalog::default())
            .view()
            .image,
        None
    );
}

#[test]
fn gallery_configuration_survives_settings_round_trip_and_save_acknowledgements() {
    let media = GallerySettings {
        selected: ImageId::new(8).unwrap(),
        fit: Fit::Cover,
        anchor: Anchor::BottomRight,
        slideshow: SlideInterval::new(86400),
    };
    let settings = Settings {
        gallery: media,
        screen: ScreenId::Gallery,
        ..Settings::default()
    };
    assert_eq!(decode(&encode(settings, 5)), Some((settings, 5)));
    let app = App {
        gallery: Gallery::new(media, catalog()),
        screen: ScreenId::Gallery,
        ..App::default()
    };
    let edited = app
        .transition(AppEvent::Gallery(GalleryEvent::Fit(Fit::Contain)))
        .state;
    let acknowledged = edited.transition(AppEvent::Configure(app.settings())).state;
    assert_eq!(acknowledged.gallery.settings().fit, Fit::Contain);
    assert_eq!(
        acknowledged.settings().gallery,
        acknowledged.gallery.settings()
    );
}

#[test]
fn daily_format_records_keep_their_content_and_default_only_gallery_preferences() {
    use device_core::{
        daily::{DailyEdit, Text, DAILY_BYTES},
        settings::{crc32, COMMIT_OFFSET},
    };
    let mut settings = Settings::default();
    settings.daily = settings.daily.edit(DailyEdit::Note(
        Text::new("Keep format 3 content.").unwrap(),
    ));
    let mut bytes = encode(settings, 21);
    bytes[4] = 3;
    bytes[16 + DAILY_BYTES + 4..COMMIT_OFFSET - 4].fill(0);
    let checksum = crc32(&bytes[..COMMIT_OFFSET - 4]);
    bytes[COMMIT_OFFSET - 4..COMMIT_OFFSET].copy_from_slice(&checksum.to_le_bytes());
    assert_eq!(decode(&bytes), Some((settings, 21)));
    bytes[7] = ScreenId::Gallery.code();
    let checksum = crc32(&bytes[..COMMIT_OFFSET - 4]);
    bytes[COMMIT_OFFSET - 4..COMMIT_OFFSET].copy_from_slice(&checksum.to_le_bytes());
    assert_eq!(decode(&bytes), None);
}

#[test]
fn a_transfer_revokes_a_queued_display_and_releases_only_after_ready_content() {
    let app = App {
        screen: ScreenId::Gallery,
        gallery: Gallery::new(GallerySettings::default(), catalog()),
        ..App::default()
    };
    let updating = app
        .transition(AppEvent::Wake(WakeReason::PowerOn))
        .state
        .transition(AppEvent::Telemetry(Default::default()))
        .state;
    assert!(matches!(updating.refresh, RefreshState::Updating { .. }));
    let transfer = updating.transition(AppEvent::Gallery(GalleryEvent::Busy(true)));
    assert!(transfer.actions.contains(&Action::StopPanel));
    assert_eq!(transfer.state.refresh, RefreshState::Idle);
    assert!(!transfer
        .actions
        .iter()
        .any(|action| matches!(action, Action::Display { .. } | Action::Sleep { .. })));
    let waiting = transfer.state.transition(AppEvent::RequestSleep);
    assert!(!waiting
        .actions
        .iter()
        .any(|action| matches!(action, Action::Sleep { .. })));
    let ready = waiting
        .state
        .transition(AppEvent::Gallery(GalleryEvent::Ready(catalog())));
    assert!(ready
        .actions
        .iter()
        .any(|action| matches!(action, Action::Display { .. })));
}

#[test]
fn gallery_short_presses_change_images_and_holds_leave_the_gallery() {
    let app = App {
        screen: ScreenId::Gallery,
        gallery: Gallery::new(GallerySettings::default(), catalog()),
        ..App::default()
    };
    let next = app.transition(AppEvent::Button(Button::Next)).state;
    assert_eq!(next.screen, ScreenId::Gallery);
    assert_eq!(next.gallery.settings().selected.number(), 3);
    let leave = app.transition(AppEvent::Hold(Button::Next)).state;
    assert_ne!(leave.screen, ScreenId::Gallery);
    let started = app.transition(AppEvent::Button(Button::Refresh)).state;
    assert!(started.gallery.settings().slideshow.is_some());
    let paused = started.transition(AppEvent::Button(Button::Refresh)).state;
    assert_eq!(paused.gallery.settings().slideshow, None);
}
