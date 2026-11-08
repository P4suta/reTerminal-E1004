use device_core::{
    app::{App, AppEvent, ScreenId},
    geometry::Rotation,
    media::*,
};

#[test]
fn automatic_slides_do_not_request_durable_preference_changes() {
    let mut records = [None; IMAGE_SLOTS];
    for (slot, number) in [1, 3].into_iter().enumerate() {
        records[slot] = Some(ImageRecord::image(
            ImageId::new(number).unwrap(),
            slot as u64,
            Rotation::Portrait,
            0,
        ));
    }
    let mut app = App {
        screen: ScreenId::Gallery,
        gallery: Gallery::new(
            GallerySettings::default(),
            ImageIndex::new(records).catalog(),
        ),
        ..Default::default()
    };
    app = app
        .transition(AppEvent::Gallery(GalleryEvent::Slideshow(
            SlideInterval::new(60),
        )))
        .state;
    let preferences = app.settings();
    app = app.transition(AppEvent::Tick(60000)).state;
    assert_eq!(app.gallery.settings().selected.number(), 3);
    assert_eq!(app.settings(), preferences);
}
