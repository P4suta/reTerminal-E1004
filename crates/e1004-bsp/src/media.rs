use crate::storage::{SlotStorage, StorageError};
use device_core::{
    geometry::{Rotation, FRAME_BYTES},
    media::*,
    settings::{crc32, Crc32},
};

pub const MEDIA_BASE: u32 = 0x800000;
pub const MEDIA_SLOT_BYTES: u32 = 0x100000;
pub const SERIAL_CHUNK_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaChunk {
    bytes: [u8; SERIAL_CHUNK_BYTES],
    length: u8,
}
impl MediaChunk {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.length)]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    Begin {
        transfer: u32,
        image: ImageId,
        rotation: Rotation,
        checksum: u32,
    },
    Data {
        transfer: u32,
        offset: usize,
        data: MediaChunk,
    },
    Commit(u32),
    Abort(u32),
    Delete(ImageId),
    Import(ImageId),
}
pub fn parse_command(value: &str) -> Option<MediaCommand> {
    let mut words = value.split_whitespace();
    let name = words.next()?;
    let number: u32 = words.next()?.parse().ok()?;
    let command = match name {
        "begin" => MediaCommand::Begin {
            transfer: number,
            image: ImageId::new(words.next()?.parse().ok()?)?,
            rotation: Rotation::from_code(words.next()?.parse().ok()?)?,
            checksum: words.next()?.parse().ok()?,
        },
        "data" => {
            let offset: usize = words.next()?.parse().ok()?;
            let hex = words.next()?.as_bytes();
            if hex.is_empty()
                || !hex.len().is_multiple_of(8)
                || hex.len() > SERIAL_CHUNK_BYTES * 2
                || !offset.is_multiple_of(4)
                || offset.checked_add(hex.len() / 2)? > FRAME_BYTES
            {
                return None;
            }
            let mut data = MediaChunk {
                bytes: [0; SERIAL_CHUNK_BYTES],
                length: (hex.len() / 2) as u8,
            };
            for (out, pair) in data.bytes.iter_mut().zip(hex.as_chunks::<2>().0) {
                let digit = |byte: u8| match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    b'A'..=b'F' => Some(byte - b'A' + 10),
                    _ => None,
                };
                *out = digit(pair[0])? * 16 + digit(pair[1])?;
                if !valid_palette_byte(*out) {
                    return None;
                }
            }
            MediaCommand::Data {
                transfer: number,
                offset,
                data,
            }
        }
        "commit" => MediaCommand::Commit(number),
        "abort" => MediaCommand::Abort(number),
        "delete" => MediaCommand::Delete(ImageId::new(u8::try_from(number).ok()?)?),
        "import" => MediaCommand::Import(ImageId::new(u8::try_from(number).ok()?)?),
        _ => return None,
    };
    words.next().is_none().then_some(command)
}

pub fn media_address(
    slot: PhysicalSlot,
    offset: usize,
    length: usize,
) -> Result<u32, StorageError> {
    if offset
        .checked_add(length)
        .is_none_or(|end| end > MEDIA_SLOT_BYTES as usize)
    {
        return Err(StorageError::Io);
    }
    Ok(MEDIA_BASE + slot.index() as u32 * MEDIA_SLOT_BYTES + offset as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaError {
    Storage(StorageError),
    Transfer(TransferError),
    Busy,
    Missing,
    Exhausted,
    Corrupt,
}
impl From<StorageError> for MediaError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}
impl From<TransferError> for MediaError {
    fn from(value: TransferError) -> Self {
        Self::Transfer(value)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MediaStore {
    index: ImageIndex,
    transfer: Option<Transfer>,
}
impl MediaStore {
    pub fn open(storage: &mut impl SlotStorage) -> Result<Self, MediaError> {
        let mut records = [None; IMAGE_SLOTS];
        for (i, record) in records.iter_mut().enumerate() {
            let mut bytes = [0; HEADER_BYTES];
            storage.media_read(
                PhysicalSlot::new(i as u8).ok_or(MediaError::Exhausted)?,
                0,
                &mut bytes,
            )?;
            *record = ImageRecord::decode(&bytes);
        }
        Ok(Self {
            index: ImageIndex::new(records),
            transfer: None,
        })
    }
    pub fn catalog(self) -> Catalog {
        self.index.catalog()
    }
    pub fn transfer(self) -> Option<Transfer> {
        self.transfer
    }
    pub fn abort(&mut self) {
        self.transfer = None;
    }
    pub fn begin(
        &mut self,
        storage: &mut impl SlotStorage,
        transfer_id: u32,
        id: ImageId,
        rotation: Rotation,
        checksum: u32,
    ) -> Result<(), MediaError> {
        if self.transfer.is_some() {
            return Err(MediaError::Busy);
        }
        let slot = self.index.staging().ok_or(MediaError::Exhausted)?;
        let record = ImageRecord::image(
            id,
            self.index.next_generation().ok_or(MediaError::Exhausted)?,
            rotation,
            checksum,
        );
        storage.media_erase(slot)?;
        storage.sync()?;
        self.transfer = Transfer::new(transfer_id, slot, record);
        Ok(())
    }
    pub fn write(
        &mut self,
        storage: &mut impl SlotStorage,
        id: u32,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), MediaError> {
        let transfer = self.transfer.ok_or(MediaError::Missing)?;
        let plan = transfer.plan(id, offset, bytes)?;
        if let WritePlan::Append(_) = plan {
            storage.media_program(transfer.slot(), HEADER_BYTES + offset, bytes)?;
        }
        let mut actual = [0; CHUNK_BYTES];
        storage.media_read(
            transfer.slot(),
            HEADER_BYTES + offset,
            &mut actual[..bytes.len()],
        )?;
        if &actual[..bytes.len()] != bytes {
            return Err(MediaError::Corrupt);
        }
        if let WritePlan::Append(next) = plan {
            self.transfer = Some(next);
        }
        Ok(())
    }
    fn verify(
        storage: &mut impl SlotStorage,
        slot: PhysicalSlot,
        record: ImageRecord,
    ) -> Result<(), MediaError> {
        let mut checksum = Crc32::new();
        let mut bytes = [0; CHUNK_BYTES];
        for offset in (0..FRAME_BYTES).step_by(CHUNK_BYTES) {
            let length = CHUNK_BYTES.min(FRAME_BYTES - offset);
            storage.media_read(slot, HEADER_BYTES + offset, &mut bytes[..length])?;
            if !bytes[..length].iter().copied().all(valid_palette_byte) {
                return Err(MediaError::Corrupt);
            }
            checksum.update(&bytes[..length]);
        }
        if checksum.finish() != record.descriptor().checksum {
            return Err(MediaError::Corrupt);
        }
        Ok(())
    }
    fn commit_record(
        &mut self,
        storage: &mut impl SlotStorage,
        slot: PhysicalSlot,
        record: ImageRecord,
    ) -> Result<(), MediaError> {
        let bytes = record.encode();
        storage.media_program(slot, 0, &bytes[..28])?;
        storage.sync()?;
        storage.media_program(slot, 28, &bytes[28..])?;
        storage.sync()?;
        let mut actual = [0; HEADER_BYTES];
        storage.media_read(slot, 0, &mut actual)?;
        if actual != bytes {
            return Err(MediaError::Corrupt);
        }
        self.index = self.index.committed(slot, record);
        Ok(())
    }
    pub fn commit(&mut self, storage: &mut impl SlotStorage, id: u32) -> Result<(), MediaError> {
        let transfer = self.transfer.ok_or(MediaError::Missing)?;
        let record = transfer.complete(id)?;
        Self::verify(storage, transfer.slot(), record)?;
        self.commit_record(storage, transfer.slot(), record)?;
        self.transfer = None;
        Ok(())
    }
    pub fn delete(
        &mut self,
        storage: &mut impl SlotStorage,
        id: ImageId,
    ) -> Result<(), MediaError> {
        if self.transfer.is_some() {
            return Err(MediaError::Busy);
        }
        let slot = self.index.staging().ok_or(MediaError::Exhausted)?;
        let record = ImageRecord::deleted(
            id,
            self.index.next_generation().ok_or(MediaError::Exhausted)?,
        );
        storage.media_erase(slot)?;
        storage.sync()?;
        self.commit_record(storage, slot, record)
    }
    pub fn load(
        self,
        storage: &mut impl SlotStorage,
        id: ImageId,
        bytes: &mut [u8],
    ) -> Result<ImageDescriptor, MediaError> {
        if bytes.len() != FRAME_BYTES {
            return Err(MediaError::Corrupt);
        }
        let (slot, record) = self
            .index
            .current(id)
            .filter(|(_, r)| !r.is_deleted())
            .ok_or(MediaError::Missing)?;
        storage.media_read(slot, HEADER_BYTES, bytes)?;
        if crc32(bytes) != record.descriptor().checksum
            || !bytes.iter().copied().all(valid_palette_byte)
        {
            return Err(MediaError::Corrupt);
        }
        Ok(record.descriptor())
    }
}
