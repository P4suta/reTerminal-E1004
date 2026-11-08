use device_core::{
    media::ImageId,
    settings::{decode, encode, Settings, SLOT_BYTES},
};
use e1004_bsp::storage::{write_retained_gallery, StorageError};

#[test]
fn every_interrupted_retained_write_is_rejected_before_magic_commits() {
    let previous = encode(Settings::default(), 0);
    let mut desired = Settings::default();
    desired.gallery.selected = ImageId::new(3).unwrap();
    let next = encode(desired, 0);
    let mut calls = 0;
    write_retained_gallery(&next, |index, _| {
        assert!(index < SLOT_BYTES / 4);
        calls += 1;
        Ok(())
    })
    .unwrap();
    for failure in 0..=calls {
        let mut bytes = previous;
        let mut count = 0;
        let result = write_retained_gallery(&next, |index, word| {
            if count == failure {
                return Err(StorageError::Io);
            }
            count += 1;
            bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
            Ok(())
        });
        if failure == calls {
            assert_eq!(result, Ok(()));
            assert_eq!(bytes, next);
        } else {
            assert_eq!(result, Err(StorageError::Io));
            assert_eq!(count, failure);
            assert_eq!(
                decode(&bytes),
                if failure == 0 {
                    decode(&previous)
                } else {
                    None
                }
            );
        }
    }
}
