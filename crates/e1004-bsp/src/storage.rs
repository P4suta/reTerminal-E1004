use device_core::settings::{encode, recover, Settings, COMMIT_BYTES, COMMIT_OFFSET, SLOT_BYTES};
pub const SETTINGS_BASE: u32 = 0x9000;
pub const FLASH_SECTOR_BYTES: u32 = 4096;
const SETTINGS_SLOTS: usize = 2;

pub fn settings_address(slot: usize, offset: usize, length: usize) -> Result<u32, StorageError> {
    if slot >= SETTINGS_SLOTS
        || offset >= SLOT_BYTES
        || offset
            .checked_add(length)
            .is_none_or(|end| end > SLOT_BYTES)
    {
        return Err(StorageError::Io);
    }
    Ok(SETTINGS_BASE + slot as u32 * FLASH_SECTOR_BYTES + offset as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError {
    Absent,
    Io,
    InvalidSettings,
}

pub trait SlotStorage {
    fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError>;
    fn write(&mut self, slot: usize, offset: usize, bytes: &[u8]) -> Result<(), StorageError>;
    fn sync(&mut self) -> Result<(), StorageError>;
}

pub fn save(storage: &mut impl SlotStorage, settings: Settings) -> Result<(), StorageError> {
    if !settings.valid() {
        return Err(StorageError::InvalidSettings);
    }
    let mut first = [0u8; SLOT_BYTES];
    let mut second = [0u8; SLOT_BYTES];
    storage.read(0, &mut first)?;
    storage.read(1, &mut second)?;
    let (generation, target) = match recover(&first, &second) {
        Some((_, generation, slot)) => (generation.wrapping_add(1), 1 - slot),
        None => (0, 0),
    };
    let bytes = encode(settings, generation);
    storage.write(target, COMMIT_OFFSET, &[0; COMMIT_BYTES])?;
    storage.sync()?;
    storage.write(target, 0, &bytes[..COMMIT_OFFSET])?;
    storage.sync()?;
    storage.write(target, COMMIT_OFFSET, &bytes[COMMIT_OFFSET..])?;
    storage.sync()
}
