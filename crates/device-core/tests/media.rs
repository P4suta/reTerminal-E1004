use device_core::{
    geometry::{Rotation, FRAME_BYTES},
    media::{
        Catalog, ImageId, ImageIndex, ImageRecord, PackedFrame, PhysicalSlot, Transfer,
        TransferError, WritePlan, HEADER_BYTES, IMAGE_SLOTS,
    },
    settings::{crc32, Crc32},
};

#[test]
fn streaming_checksum_matches_known_answers_and_every_partition() {
    let bytes = b"123456789";
    assert_eq!(crc32(bytes), 0xcbf4_3926);
    for split in 0..=bytes.len() {
        let mut checksum = Crc32::new();
        checksum.update(&bytes[..split]);
        checksum.update(&[]);
        checksum.update(&bytes[split..]);
        assert_eq!(checksum.finish(), crc32(bytes));
    }
}

#[test]
fn committed_headers_have_exact_fields_and_reject_every_damaged_bit() {
    for id in 1..=8 {
        let id = ImageId::new(id).unwrap();
        for rotation in [
            Rotation::Portrait,
            Rotation::Landscape,
            Rotation::InvertedPortrait,
            Rotation::InvertedLandscape,
        ] {
            for generation in [0, 1, u32::MAX as u64, u64::MAX] {
                let record = ImageRecord::image(id, generation, rotation, 0x12345678);
                let bytes = record.encode();
                assert_eq!(ImageRecord::decode(&bytes), Some(record));
                assert_eq!(ImageRecord::decode(&bytes[..HEADER_BYTES - 1]), None);
                for position in 0..HEADER_BYTES {
                    for bit in 0..8 {
                        let mut damaged = bytes;
                        damaged[position] ^= 1 << bit;
                        assert_eq!(ImageRecord::decode(&damaged), None);
                    }
                }
                let removed = ImageRecord::deleted(id, generation);
                assert_eq!(ImageRecord::decode(&removed.encode()), Some(removed));
            }
        }
    }
}

#[test]
fn a_full_collection_always_has_a_staging_slot_and_deleted_images_do_not_reappear() {
    let mut records = [None; IMAGE_SLOTS];
    for number in 1..=8 {
        records[usize::from(number - 1)] = Some(ImageRecord::image(
            ImageId::new(number).unwrap(),
            u64::from(number),
            Rotation::Portrait,
            u32::from(number),
        ));
    }
    let full = ImageIndex::new(records);
    assert_eq!(full.catalog().len(), 8);
    assert_eq!(full.staging().unwrap().index(), 8);
    assert_eq!(full.next_generation(), Some(9));
    let first = ImageId::default();
    records[8] = Some(ImageRecord::deleted(first, 9));
    let removed = ImageIndex::new(records);
    assert_eq!(removed.catalog().len(), 7);
    assert_eq!(removed.catalog().get(first), None);
    assert_eq!(removed.staging().unwrap().index(), 0);
    assert_eq!(removed.catalog().step(first, true).unwrap().number(), 2);
    assert_eq!(removed.catalog().step(first, false).unwrap().number(), 8);
    records[0] = Some(ImageRecord::image(first, 10, Rotation::Landscape, 9));
    let restored = ImageIndex::new(records);
    assert_eq!(restored.catalog().len(), 8);
    assert_eq!(restored.staging().unwrap().index(), 8);
    records[0] = Some(ImageRecord::image(first, u64::MAX, Rotation::Landscape, 9));
    assert_eq!(ImageIndex::new(records).next_generation(), None);
    assert!(PhysicalSlot::new(9).is_none());
    assert!(ImageId::new(0).is_none() && ImageId::new(9).is_none());
    assert_eq!(Catalog::default().first(), None);
}

#[test]
fn transfer_plans_reject_wrong_identity_offsets_palette_and_incomplete_content() {
    let bytes = vec![0x11; FRAME_BYTES];
    let record = ImageRecord::image(ImageId::default(), 1, Rotation::Portrait, crc32(&bytes));
    let transfer = Transfer::new(7, PhysicalSlot::new(0).unwrap(), record).unwrap();
    assert_eq!(transfer.complete(7), Err(TransferError::Incomplete));
    assert_eq!(
        transfer.plan(8, 0, &[0x11; 64]),
        Err(TransferError::Identity)
    );
    assert_eq!(transfer.plan(7, 4, &[0x11; 64]), Err(TransferError::Offset));
    assert_eq!(
        transfer.plan(7, 0, &[0x44; 64]),
        Err(TransferError::Palette)
    );
    assert_eq!(transfer.plan(7, 0, &[0x11; 3]), Err(TransferError::Length));
    let mut current = transfer;
    assert_eq!(transfer.plan(7, 0, &[]), Err(TransferError::Length));
    let partially_written = match transfer.plan(7, 0, &[0x11; 64]).unwrap() {
        WritePlan::Append(next) => next,
        _ => unreachable!(),
    };
    assert_eq!(
        partially_written.plan(7, 60, &[0x11; 8]),
        Err(TransferError::Offset)
    );
    for (chunk, bytes) in bytes.chunks(256).enumerate() {
        current = match current.plan(7, chunk * 256, bytes).unwrap() {
            WritePlan::Append(next) => next,
            WritePlan::Retry => panic!("new content became a retry"),
        };
    }
    assert_eq!(current.complete(7), Ok(record));
    assert_eq!(current.plan(7, 0, &[0x11; 64]), Ok(WritePlan::Retry));
    assert_eq!(
        current.plan(7, FRAME_BYTES, &[0x11; 4]),
        Err(TransferError::Length)
    );
}

#[test]
fn packed_frames_require_a_complete_palette_and_a_matching_file_checksum() {
    let bytes = vec![0x11; FRAME_BYTES];
    let frame = PackedFrame::new(Rotation::Landscape, &bytes).unwrap();
    let record = ImageRecord::image(
        ImageId::default(),
        0,
        frame.rotation(),
        crc32(frame.bytes()),
    );
    let mut file = Vec::from(record.encode());
    file.extend_from_slice(frame.bytes());
    let decoded = PackedFrame::from_file(&file).unwrap();
    assert_eq!(decoded.rotation(), Rotation::Landscape);
    assert_eq!(decoded.bytes(), bytes);
    for index in [HEADER_BYTES, HEADER_BYTES + FRAME_BYTES / 2, file.len() - 1] {
        file[index] ^= 1;
        assert!(PackedFrame::from_file(&file).is_none());
        file[index] ^= 1;
    }
    assert!(PackedFrame::new(Rotation::Portrait, &bytes[..FRAME_BYTES - 1]).is_none());
    for byte in 0..=u8::MAX {
        assert_eq!(
            device_core::media::valid_palette_byte(byte),
            [0, 1, 2, 3, 5, 6].contains(&(byte >> 4)) && [0, 1, 2, 3, 5, 6].contains(&(byte & 15))
        );
    }
}
