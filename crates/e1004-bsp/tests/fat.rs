use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx, TimeSource, Timestamp};
use std::{
    cell::{Cell, RefCell},
    io::{self, Cursor, Read},
};

struct Disk {
    bytes: RefCell<Vec<u8>>,
    removed: Cell<bool>,
    writes: Cell<usize>,
    remove_at_write: Option<usize>,
}
impl BlockDevice for &Disk {
    type Error = io::Error;
    fn read(&self, blocks: &mut [Block], start: BlockIdx) -> Result<(), Self::Error> {
        if self.removed.get() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        for (index, block) in blocks.iter_mut().enumerate() {
            let offset = (start.0 as usize + index) * 512;
            block.contents.copy_from_slice(
                self.bytes
                    .borrow()
                    .get(offset..offset + 512)
                    .ok_or(io::ErrorKind::UnexpectedEof)?,
            );
        }
        Ok(())
    }
    fn write(&self, blocks: &[Block], start: BlockIdx) -> Result<(), Self::Error> {
        self.writes.set(self.writes.get() + 1);
        if self.remove_at_write == Some(self.writes.get()) {
            self.removed.set(true);
        }
        if self.removed.get() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        for (index, block) in blocks.iter().enumerate() {
            let offset = (start.0 as usize + index) * 512;
            self.bytes
                .borrow_mut()
                .get_mut(offset..offset + 512)
                .ok_or(io::ErrorKind::WriteZero)?
                .copy_from_slice(&block.contents);
        }
        Ok(())
    }
    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        Ok(BlockCount((self.bytes.borrow().len() / 512) as u32))
    }
}
struct Clock;
impl TimeSource for Clock {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 56,
            zero_indexed_month: 10,
            zero_indexed_day: 4,
            hours: 19,
            minutes: 30,
            seconds: 0,
        }
    }
}

#[test]
fn fat32_writes_are_readable_by_an_independent_filesystem_and_removal_is_an_error() {
    let mut bytes = vec![0u8; 64 * 1024 * 1024];
    let start = 2048usize * 512;
    let sectors = (bytes.len() / 512 - 2048) as u32;
    bytes[446 + 4] = 0x0c;
    bytes[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
    bytes[446 + 12..446 + 16].copy_from_slice(&sectors.to_le_bytes());
    bytes[510..512].copy_from_slice(&[0x55, 0xaa]);
    fatfs::format_volume(
        Cursor::new(&mut bytes[start..]),
        fatfs::FormatVolumeOptions::new().fat_type(fatfs::FatType::Fat32),
    )
    .unwrap();
    let disk = Disk {
        bytes: RefCell::new(bytes),
        removed: Cell::new(false),
        writes: Cell::new(0),
        remove_at_write: None,
    };
    let count = e1004_bsp::fat::diagnostic(&disk, Clock).unwrap();
    let mut bytes = disk.bytes.borrow_mut();
    let volume =
        fatfs::FileSystem::new(Cursor::new(&mut bytes[start..]), fatfs::FsOptions::new()).unwrap();
    let mut file = volume.root_dir().open_file("E1004.TXT").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    assert_eq!(contents, "reTerminal E1004 offline storage diagnostic\r\n");
    assert_eq!(count, contents.len());
    drop(file);
    drop(volume);
    drop(bytes);
    let original = disk.bytes.borrow().clone();
    disk.writes.set(0);
    e1004_bsp::fat::diagnostic(&disk, Clock).unwrap();
    let write_count = disk.writes.get();
    assert!(write_count > 0);
    for failure in 1..=write_count {
        let interrupted = Disk {
            bytes: RefCell::new(original.clone()),
            removed: Cell::new(false),
            writes: Cell::new(0),
            remove_at_write: Some(failure),
        };
        assert!(
            e1004_bsp::fat::diagnostic(&interrupted, Clock).is_err(),
            "removal at write {failure} falsely succeeded"
        );
    }
    disk.removed.set(true);
    assert!(e1004_bsp::fat::diagnostic(&disk, Clock).is_err());
}
