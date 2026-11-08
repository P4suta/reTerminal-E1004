use device_core::{
    geometry::{Rotation, FRAME_BYTES},
    media::{ImageId, ImageRecord},
    settings::crc32,
};
use device_preview::Preview;
fn file(value: u8) -> Vec<u8> {
    let data = vec![value; FRAME_BYTES];
    let mut file = ImageRecord::image(ImageId::default(), 0, Rotation::Portrait, crc32(&data))
        .encode()
        .to_vec();
    file.extend(data);
    file
}
#[test]
fn imported_images_are_atomic_and_only_matching_display_success_replaces_pixels() {
    let mut preview = Preview::default();
    preview.import_image(1, &file(0x33)).unwrap();
    assert!(preview.frame().iter().all(|v| *v == 0x33));
    preview.command("wait").unwrap();
    preview.import_image(1, &file(0x55)).unwrap();
    assert!(preview.frame().iter().all(|v| *v == 0x33));
    preview.command("timeout").unwrap();
    assert!(preview.frame().iter().all(|v| *v == 0x33));
    preview.command("hold refresh").unwrap();
    preview.command("complete").unwrap();
    assert!(preview.frame().iter().all(|v| *v == 0x55));
    let old = preview.frame();
    let mut invalid = file(0x22);
    invalid[40] ^= 1;
    assert!(preview.import_image(1, &invalid).is_err());
    assert_eq!(preview.frame(), old);
    let exported = preview.export_image(1).unwrap();
    let mut restored = Preview::default();
    restored.import_image(1, &exported).unwrap();
    assert_eq!(restored.frame(), old);
}
