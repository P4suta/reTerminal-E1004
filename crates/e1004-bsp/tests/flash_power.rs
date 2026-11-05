use device_core::settings::{encode, recover, Settings, SLOT_BYTES};
use e1004_bsp::storage::{save, settings_address, SlotStorage, StorageError};
const SECTOR: usize = 4096;
const PHYSICAL_WRITES: usize = 3;

#[derive(Clone)]
struct Flash {
    sectors: [[u8; SECTOR]; 2],
    remaining: Option<usize>,
}
impl SlotStorage for Flash {
    fn read(&mut self, slot: usize, output: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
        settings_address(slot, 0, output.len())?;
        output.copy_from_slice(&self.sectors[slot][..SLOT_BYTES]);
        Ok(())
    }
    fn write(&mut self, slot: usize, offset: usize, bytes: &[u8]) -> Result<(), StorageError> {
        settings_address(slot, offset, bytes.len())?;
        let mut sector = self.sectors[slot];
        sector[offset..offset + bytes.len()].copy_from_slice(bytes);
        let available = self.remaining.unwrap_or(SECTOR * 2).min(SECTOR * 2);
        self.sectors[slot][..available.min(SECTOR)].fill(0xff);
        if available >= SECTOR {
            for (target, byte) in self.sectors[slot]
                .iter_mut()
                .zip(sector)
                .take(available - SECTOR)
            {
                *target &= byte;
            }
        }
        if let Some(remaining) = self.remaining.as_mut() {
            if *remaining < SECTOR * 2 {
                return Err(StorageError::Io);
            }
            *remaining -= SECTOR * 2;
        }
        Ok(())
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        Ok(())
    }
}

#[test]
fn every_erase_and_program_byte_interruption_preserves_a_committed_setting() {
    let previous = Settings::default();
    let desired = Settings {
        refresh_seconds: 1800,
        rotation: 3,
    };
    for generation in [7, u32::MAX] {
        let mut flash = Flash {
            sectors: [[0xff; SECTOR]; 2],
            remaining: None,
        };
        flash.sectors[0][..SLOT_BYTES].copy_from_slice(&encode(previous, generation));
        flash.sectors[1][..SLOT_BYTES]
            .copy_from_slice(&encode(previous, generation.wrapping_sub(1)));
        for cut in 0..=PHYSICAL_WRITES * SECTOR * 2 {
            let mut interrupted = flash.clone();
            interrupted.remaining = Some(cut);
            let result = save(&mut interrupted, desired);
            assert_eq!(
                interrupted.sectors[0], flash.sectors[0],
                "active sector changed at {cut}"
            );
            let recovered = recover(
                &interrupted.sectors[0][..SLOT_BYTES],
                &interrupted.sectors[1][..SLOT_BYTES],
            )
            .unwrap()
            .0;
            assert!(recovered == previous || recovered == desired, "cut={cut}");
            if result.is_ok() {
                assert_eq!(recovered, desired);
            }
        }
    }
}

#[test]
fn flash_ranges_and_reserved_partition_match_the_production_adapter() {
    assert_eq!(settings_address(0, 0, SLOT_BYTES), Ok(0x9000));
    assert_eq!(settings_address(1, 23, 1), Ok(0xa017));
    for (slot, offset, length) in [
        (2, 0, 1),
        (0, 24, 0),
        (1, 23, 2),
        (0, usize::MAX, 1),
        (0, 1, usize::MAX),
    ] {
        assert_eq!(
            settings_address(slot, offset, length),
            Err(StorageError::Io)
        );
    }
    let partitions = include_str!("../../../firmware/partitions.csv");
    for expected in [
        ["settings_a", "0x40", "0x00", "0x9000", "0x1000"],
        ["settings_b", "0x40", "0x01", "0xa000", "0x1000"],
    ] {
        assert!(partitions
            .lines()
            .any(|line| line.split(',').map(str::trim).collect::<Vec<_>>() == expected));
    }
}
