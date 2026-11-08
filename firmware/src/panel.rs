use device_core::telemetry::StorageStatus;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::{
    delay::Delay,
    gpio::{Input, Output},
    rtc_cntl::sleep::{LowPower, RtcSleepConfig},
    spi::master::Spi,
    Blocking,
};
use t133a01::{Error, PanelIo, Target};

const SD_POWER_SETTLE_MS: u32 = 5;
const UNIX_EPOCH_YEAR: u16 = 1970;
const DATA_START_COMMAND: u8 = 0x10;
const BUSY_POLL_MS: u64 = 10;
const FAT_DEFAULT_TIMESTAMP: embedded_sdmmc::Timestamp = embedded_sdmmc::Timestamp {
    year_since_1970: 10,
    zero_indexed_month: 0,
    zero_indexed_day: 0,
    hours: 0,
    minutes: 0,
    seconds: 0,
};

pub struct Hardware {
    pub spi: Spi<'static, Blocking>,
    pub left: Output<'static>,
    pub right: Output<'static>,
    pub dc: Output<'static>,
    pub reset: Output<'static>,
    pub enable: Output<'static>,
    pub busy: Input<'static>,
    pub sd_cs: Output<'static>,
    pub sd_enable: Output<'static>,
    pub sd_detect: Input<'static>,
    pub low_power: LowPower<'static>,
}
impl Hardware {
    fn deselect(&mut self) {
        self.left.set_high();
        self.right.set_high();
        self.sd_cs.set_high();
    }
    fn select(&mut self, target: Target) {
        self.deselect();
        match target {
            Target::Left => self.left.set_low(),
            Target::Right => self.right.set_low(),
            Target::Both => {
                self.left.set_low();
                self.right.set_low();
            }
        }
    }
    pub fn stop(&mut self) {
        self.deselect();
        self.enable.set_high();
    }
    pub fn sleep(&mut self, seconds: u32) {
        self.stop();
        self.sd_enable.set_low();
        if seconds == 0 {
            self.low_power.clear_wakeup_deadline();
        } else {
            self.low_power.set_wakeup_deadline(
                esp_hal::time::Instant::now()
                    + esp_hal::time::Duration::from_secs(u64::from(seconds)),
            );
        }
        let mut config = RtcSleepConfig::default();
        config.set_rtc_fastmem_pd_en(false);
        self.low_power.sleep_deep_with_rejection(config);
    }
    pub fn sd_diagnose(&mut self) -> StorageStatus {
        if self.sd_detect.is_high() {
            return StorageStatus::Absent;
        }
        self.deselect();
        self.sd_enable.set_high();
        let mut delay = Delay::new();
        embedded_hal::delay::DelayNs::delay_ms(&mut delay, SD_POWER_SETTLE_MS);
        let device = match embedded_hal_bus::spi::ExclusiveDevice::new(
            &mut self.spi,
            &mut self.sd_cs,
            delay,
        ) {
            Ok(device) => device,
            Err(_) => return StorageStatus::Fault,
        };
        let card = embedded_sdmmc::SdCard::new(device, Delay::new());
        match e1004_bsp::fat::diagnostic(card, DiagnosticClock) {
            Ok(_) => StorageStatus::Ready,
            Err(_) => StorageStatus::Fault,
        }
    }
}

struct DiagnosticClock;
impl embedded_sdmmc::TimeSource for DiagnosticClock {
    fn get_timestamp(&self) -> embedded_sdmmc::Timestamp {
        let time = crate::snapshot().and_then(|app| match app.telemetry.clock {
            device_core::telemetry::Sample::Fresh { value, .. } => Some(value),
            _ => None,
        });
        match time {
            Some(time) => embedded_sdmmc::Timestamp {
                year_since_1970: (time.year - UNIX_EPOCH_YEAR) as u8,
                zero_indexed_month: time.month - 1,
                zero_indexed_day: time.day - 1,
                hours: time.hour,
                minutes: time.minute,
                seconds: time.second,
            },
            None => FAT_DEFAULT_TIMESTAMP,
        }
    }
}

impl PanelIo for Hardware {
    async fn cooperate(&mut self) {
        embassy_futures::yield_now().await;
    }
    fn enable(&mut self, on: bool) -> Result<(), Error> {
        self.enable.set_level(if on {
            esp_hal::gpio::Level::Low
        } else {
            esp_hal::gpio::Level::High
        });
        Ok(())
    }
    fn reset(&mut self, high: bool) -> Result<(), Error> {
        self.reset.set_level(if high {
            esp_hal::gpio::Level::High
        } else {
            esp_hal::gpio::Level::Low
        });
        Ok(())
    }
    fn command(&mut self, target: Target, command: u8, data: &[u8]) -> Result<(), Error> {
        self.select(target);
        self.dc.set_low();
        let result = self.spi.write(&[command]).and_then(|_| {
            self.dc.set_high();
            self.spi.write(data)
        });
        self.deselect();
        result.map_err(|_| Error::Transport)
    }
    fn begin_data(&mut self, target: Target) -> Result<(), Error> {
        self.select(target);
        self.dc.set_low();
        let result = self.spi.write(&[DATA_START_COMMAND]);
        self.dc.set_high();
        if result.is_err() {
            self.deselect();
        }
        result.map_err(|_| Error::Transport)
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.spi.write(bytes).map_err(|_| Error::Transport)
    }
    fn end_data(&mut self) -> Result<(), Error> {
        self.deselect();
        Ok(())
    }
    async fn delay_ms(&mut self, milliseconds: u32) {
        Timer::after(Duration::from_millis(u64::from(milliseconds))).await;
    }
    async fn wait_ready(&mut self, timeout_ms: u32) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        loop {
            Timer::after(Duration::from_millis(BUSY_POLL_MS)).await;
            if self.busy.is_high() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::BusyTimeout);
            }
        }
    }
}

impl device_runtime::tasks::BusIo for Hardware {
    fn stop(&mut self) {
        Hardware::stop(self);
    }
    fn sd_diagnose(&mut self) -> StorageStatus {
        Hardware::sd_diagnose(self)
    }
    async fn sd_import(
        &mut self,
        id: device_core::media::ImageId,
        bytes: &mut [u8],
    ) -> Result<device_core::media::ImageDescriptor, e1004_bsp::storage::StorageError> {
        use e1004_bsp::storage::StorageError;
        if self.sd_detect.is_high() {
            return Err(StorageError::Absent);
        }
        self.deselect();
        self.sd_enable.set_high();
        let mut delay = Delay::new();
        embedded_hal::delay::DelayNs::delay_ms(&mut delay, SD_POWER_SETTLE_MS);
        let device =
            embedded_hal_bus::spi::ExclusiveDevice::new(&mut self.spi, &mut self.sd_cs, delay)
                .map_err(|_| StorageError::Io)?;
        let card = embedded_sdmmc::SdCard::new(device, Delay::new());
        e1004_bsp::fat::read_image(card, DiagnosticClock, id, bytes, embassy_futures::yield_now)
            .await
            .map_err(|_| StorageError::Io)
    }
    fn deep_sleep(&mut self, seconds: u32) -> device_runtime::tasks::SleepOutcome {
        self.sleep(seconds);
        device_runtime::tasks::SleepOutcome::Rejected
    }
}
