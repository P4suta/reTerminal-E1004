use device_core::{
    geometry::{Rotation, FRAME_BYTES},
    media::*,
    settings::crc32,
};
use e1004_bsp::{
    media::{media_address, MediaStore, MEDIA_SLOT_BYTES},
    storage::{SlotStorage, StorageError},
};

struct Flash {
    bytes: Vec<u8>,
    fail: Option<usize>,
    calls: usize,
}
impl Flash {
    fn operation(&mut self) -> Result<(), StorageError> {
        self.calls += 1;
        if self.fail == Some(self.calls) {
            Err(StorageError::Io)
        } else {
            Ok(())
        }
    }
}
impl Default for Flash {
    fn default() -> Self {
        Self {
            bytes: vec![255; MEDIA_SLOT_BYTES as usize * IMAGE_SLOTS],
            fail: None,
            calls: 0,
        }
    }
}
impl SlotStorage for Flash {
    fn read(
        &mut self,
        _: usize,
        _: &mut [u8; device_core::settings::SLOT_BYTES],
    ) -> Result<(), StorageError> {
        Err(StorageError::Absent)
    }
    fn write(&mut self, _: usize, _: usize, _: &[u8]) -> Result<(), StorageError> {
        Err(StorageError::Absent)
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        self.operation()
    }
    fn media_read(
        &mut self,
        slot: PhysicalSlot,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), StorageError> {
        self.operation()?;
        let start = slot.index() * MEDIA_SLOT_BYTES as usize + offset;
        out.copy_from_slice(&self.bytes[start..start + out.len()]);
        Ok(())
    }
    fn media_erase(&mut self, slot: PhysicalSlot) -> Result<(), StorageError> {
        self.operation()?;
        let start = slot.index() * MEDIA_SLOT_BYTES as usize;
        self.bytes[start..start + MEDIA_SLOT_BYTES as usize].fill(255);
        Ok(())
    }
    fn media_program(
        &mut self,
        slot: PhysicalSlot,
        offset: usize,
        data: &[u8],
    ) -> Result<(), StorageError> {
        self.operation()?;
        assert!(offset.is_multiple_of(4) && data.len().is_multiple_of(4));
        let start = slot.index() * MEDIA_SLOT_BYTES as usize + offset;
        for (old, new) in self.bytes[start..start + data.len()].iter_mut().zip(data) {
            assert_eq!(*old & new, *new);
            *old &= new;
        }
        Ok(())
    }
}

fn upload(store: &mut MediaStore, flash: &mut Flash, id: ImageId, value: u8) {
    let data = vec![value; FRAME_BYTES];
    store
        .begin(flash, 42, id, Rotation::Landscape, crc32(&data))
        .unwrap();
    for (i, chunk) in data.chunks(CHUNK_BYTES).enumerate() {
        store.write(flash, 42, i * CHUNK_BYTES, chunk).unwrap();
    }
    store.commit(flash, 42).unwrap();
}

#[test]
fn raw_nor_transactions_verify_retries_and_commit_only_complete_images() {
    let mut flash = Flash::default();
    let mut store = MediaStore::open(&mut flash).unwrap();
    let id = ImageId::default();
    let data = vec![0x33; FRAME_BYTES];
    store
        .begin(&mut flash, 7, id, Rotation::Portrait, crc32(&data))
        .unwrap();
    store.write(&mut flash, 7, 0, &data[..256]).unwrap();
    store.write(&mut flash, 7, 0, &data[..256]).unwrap();
    assert!(store.write(&mut flash, 7, 0, &[0x11; 256]).is_err());
    assert!(store.commit(&mut flash, 7).is_err());
    assert!(MediaStore::open(&mut flash).unwrap().catalog().is_empty());
    store.abort();
    upload(&mut store, &mut flash, id, 0x55);
    let mut out = vec![0; FRAME_BYTES];
    assert_eq!(
        store.load(&mut flash, id, &mut out).unwrap().rotation,
        Rotation::Landscape
    );
    assert!(out.iter().all(|v| *v == 0x55));
    store.delete(&mut flash, id).unwrap();
    assert!(MediaStore::open(&mut flash).unwrap().catalog().is_empty());
}

#[test]
fn interrupted_replacements_preserve_committed_content_and_slots_are_bounded() {
    let mut flash = Flash::default();
    let mut store = MediaStore::open(&mut flash).unwrap();
    let id = ImageId::default();
    upload(&mut store, &mut flash, id, 0x22);
    for fail in 1..=5 {
        flash.calls = 0;
        flash.fail = Some(fail);
        let mut candidate = store;
        let result = candidate.begin(&mut flash, 43, id, Rotation::Portrait, 0);
        if result.is_ok() {
            let _ = candidate.write(&mut flash, 43, 0, &[0; 256]);
        }
        flash.fail = None;
        let recovered = MediaStore::open(&mut flash).unwrap();
        assert_eq!(recovered.catalog(), store.catalog());
        let mut out = vec![0; FRAME_BYTES];
        recovered.load(&mut flash, id, &mut out).unwrap();
        assert!(out.iter().all(|v| *v == 0x22));
    }
    for s in 0..IMAGE_SLOTS as u8 {
        let slot = PhysicalSlot::new(s).unwrap();
        assert_eq!(
            media_address(slot, 0, HEADER_BYTES).unwrap(),
            0x800000 + u32::from(s) * MEDIA_SLOT_BYTES
        );
        assert!(media_address(slot, MEDIA_SLOT_BYTES as usize - 1, 2).is_err());
        assert!(media_address(slot, usize::MAX, 1).is_err());
    }
}
