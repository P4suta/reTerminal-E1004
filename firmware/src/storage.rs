use device_core::settings::SLOT_BYTES;
use e1004_bsp::storage::{settings_address, SlotStorage, StorageError};
use portable_atomic::{AtomicU32, Ordering};

#[esp_hal::ram(unstable(rtc_fast, persistent))]
static GALLERY_RETAINED: [AtomicU32; SLOT_BYTES / 4] =
    [const { AtomicU32::new(0) }; SLOT_BYTES / 4];

pub struct FlashSlots {
    flash: esp_storage::FlashStorage<'static>,
}
impl FlashSlots {
    pub fn new(flash: esp_storage::FlashStorage<'static>) -> Self {
        Self { flash }
    }
}
impl SlotStorage for FlashSlots {
    fn read_retained_gallery(&mut self) -> Option<[u8; SLOT_BYTES]> {
        let mut bytes = [0; SLOT_BYTES];
        for (word, target) in GALLERY_RETAINED.iter().zip(bytes.as_chunks_mut::<4>().0) {
            *target = word.load(Ordering::SeqCst).to_le_bytes();
        }
        Some(bytes)
    }
    fn retain_gallery(&mut self, bytes: &[u8; SLOT_BYTES]) {
        let _ = e1004_bsp::storage::write_retained_gallery(bytes, |index, word| {
            GALLERY_RETAINED[index].store(word, Ordering::SeqCst);
            Ok(())
        });
    }
    fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
        self.flash
            .read(settings_address(slot, 0, bytes.len())?, bytes)
            .map_err(|_| StorageError::Io)
    }
    fn write(&mut self, slot: usize, offset: usize, bytes: &[u8]) -> Result<(), StorageError> {
        self.flash
            .write(settings_address(slot, offset, bytes.len())?, bytes)
            .map_err(|_| StorageError::Io)
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        Ok(())
    }
    fn media_read(
        &mut self,
        slot: device_core::media::PhysicalSlot,
        offset: usize,
        bytes: &mut [u8],
    ) -> Result<(), StorageError> {
        self.flash
            .read(
                e1004_bsp::media::media_address(slot, offset, bytes.len())?,
                bytes,
            )
            .map_err(|_| StorageError::Io)
    }
    fn media_program(
        &mut self,
        slot: device_core::media::PhysicalSlot,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), StorageError> {
        if !offset.is_multiple_of(4) || !bytes.len().is_multiple_of(4) {
            return Err(StorageError::Io);
        }
        self.flash
            .write_nor(
                e1004_bsp::media::media_address(slot, offset, bytes.len())?,
                bytes,
            )
            .map_err(|_| StorageError::Io)
    }
    fn media_erase(&mut self, slot: device_core::media::PhysicalSlot) -> Result<(), StorageError> {
        let start = e1004_bsp::media::media_address(slot, 0, 0)?;
        self.flash
            .erase(start, start + e1004_bsp::media::MEDIA_SLOT_BYTES)
            .map_err(|_| StorageError::Io)
    }
}
