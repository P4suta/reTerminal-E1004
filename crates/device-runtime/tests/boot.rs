use device_core::{
    settings::{encode, Settings, SLOT_BYTES},
    telemetry::Fault,
};
use device_runtime::tasks::boot_settings;
use e1004_bsp::storage::{SlotStorage, StorageError};

struct Flash {
    slots: [[u8; SLOT_BYTES]; 2],
    failures: u8,
    contaminate: bool,
    reads: Vec<usize>,
}
impl SlotStorage for Flash {
    fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
        self.reads.push(slot);
        if self.failures & (1 << slot) != 0 {
            if self.contaminate {
                *bytes = encode(
                    Settings {
                        refresh_seconds: 86400,
                        rotation: 2,
                        ..Settings::default()
                    },
                    500,
                );
            }
            return Err(StorageError::Io);
        }
        *bytes = self.slots[slot];
        Ok(())
    }
    fn write(&mut self, _: usize, _: usize, _: &[u8]) -> Result<(), StorageError> {
        panic!("boot must never write flash")
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        panic!("boot must never modify flash")
    }
}
#[test]
fn boot_reads_both_slots_and_preserves_the_readable_setting_without_trusting_failed_buffers() {
    let previous = Settings {
        refresh_seconds: 600,
        rotation: 1,
        ..Settings::default()
    };
    let desired = Settings {
        refresh_seconds: 1800,
        rotation: 3,
        ..Settings::default()
    };
    for failures in 0..4 {
        for contaminate in [false, true] {
            let mut flash = Flash {
                slots: [encode(previous, 4), encode(desired, 5)],
                failures,
                contaminate,
                reads: Vec::new(),
            };
            let expected = match failures {
                0 | 1 => desired,
                2 => previous,
                _ => Settings::default(),
            };
            assert_eq!(
                boot_settings(&mut flash),
                (
                    expected,
                    if failures == 0 {
                        None
                    } else {
                        Some(Fault::SettingsStorage)
                    }
                )
            );
            assert_eq!(flash.reads, [0, 1]);
        }
    }
}
