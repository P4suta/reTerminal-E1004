use device_core::settings::SLOT_BYTES;
use e1004_bsp::storage::{settings_address, SlotStorage, StorageError};

pub struct FlashSlots {
    flash: esp_storage::FlashStorage<'static>,
}
impl FlashSlots {
    pub fn new(flash: esp_storage::FlashStorage<'static>) -> Self {
        Self { flash }
    }
}
impl SlotStorage for FlashSlots {
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
}
