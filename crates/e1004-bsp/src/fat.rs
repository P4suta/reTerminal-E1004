use embedded_sdmmc::{BlockDevice, Error, Mode, TimeSource, VolumeIdx, VolumeManager};

pub const DIAGNOSTIC_CONTENT: &[u8] = b"reTerminal E1004 offline storage diagnostic\r\n";

pub async fn read_image<D: BlockDevice, T: TimeSource, F: core::future::Future<Output = ()>>(
    device: D,
    clock: T,
    id: device_core::media::ImageId,
    bytes: &mut [u8],
    mut cooperate: impl FnMut() -> F,
) -> Result<device_core::media::ImageDescriptor, Error<D::Error>> {
    use device_core::{
        geometry::FRAME_BYTES,
        media::{valid_palette_byte, ImageRecord, HEADER_BYTES},
        settings::crc32,
    };
    if bytes.len() != FRAME_BYTES {
        return Err(Error::FormatError("invalid frame buffer"));
    }
    let manager = VolumeManager::new(device, clock);
    let volume = manager.open_volume(VolumeIdx(0))?;
    let root = volume.open_root_dir()?;
    let mut name = *b"IMAGE1.EPD";
    name[5] = b'0' + id.number();
    let name = core::str::from_utf8(&name).map_err(|_| Error::FormatError("invalid image name"))?;
    let file = root.open_file_in_dir(name, Mode::ReadOnly)?;
    if file.length() as usize != HEADER_BYTES + FRAME_BYTES {
        return Err(Error::FormatError("invalid image length"));
    }
    let mut header = [0; HEADER_BYTES];
    if file.read(&mut header)? != HEADER_BYTES {
        return Err(Error::FormatError("truncated image header"));
    }
    let record = ImageRecord::decode(&header)
        .filter(|record| !record.is_deleted())
        .ok_or(Error::FormatError("invalid image header"))?;
    for chunk in bytes.chunks_mut(4096) {
        if file.read(chunk)? != chunk.len() {
            return Err(Error::FormatError("truncated image pixels"));
        }
        cooperate().await;
    }
    file.close()?;
    root.close()?;
    volume.close()?;
    if crc32(bytes) != record.descriptor().checksum
        || !bytes.iter().copied().all(valid_palette_byte)
    {
        return Err(Error::FormatError("invalid image pixels"));
    }
    Ok(record.descriptor())
}

pub fn diagnostic<D: BlockDevice, T: TimeSource>(
    device: D,
    clock: T,
) -> Result<usize, Error<D::Error>> {
    let manager = VolumeManager::new(device, clock);
    let volume = manager.open_volume(VolumeIdx(0))?;
    let root = volume.open_root_dir()?;
    let file = root.open_file_in_dir("E1004.TXT", Mode::ReadWriteCreateOrTruncate)?;
    file.write(DIAGNOSTIC_CONTENT)?;
    file.flush()?;
    file.close()?;
    let file = root.open_file_in_dir("E1004.TXT", Mode::ReadOnly)?;
    let mut contents = [0; DIAGNOSTIC_CONTENT.len() + 1];
    let count = file.read(&mut contents)?;
    file.close()?;
    root.close()?;
    volume.close()?;
    if &contents[..count] != DIAGNOSTIC_CONTENT {
        return Err(Error::FormatError("diagnostic readback differs from write"));
    }
    Ok(count)
}
