use embedded_sdmmc::{BlockDevice, Error, Mode, TimeSource, VolumeIdx, VolumeManager};

pub const DIAGNOSTIC_CONTENT: &[u8] = b"reTerminal E1004 offline storage diagnostic\r\n";

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
