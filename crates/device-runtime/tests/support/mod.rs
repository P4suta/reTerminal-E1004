use device_core::{
    geometry::{FRAME_BYTES, HEIGHT},
    settings::{Settings, SLOT_BYTES},
    telemetry::*,
};
use device_runtime::{
    buttons::ButtonsIo,
    console::ConsoleIo,
    measurements::{IoFailure, MeasurementIo},
    tasks::{BusIo, SleepOutcome},
};
use e1004_bsp::storage::{SlotStorage, StorageError};
use embassy_time::{Duration, Instant, MockDriver, Timer};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};
use t133a01::{Error, PanelIo, Target};
#[cfg(feature = "ble")]
pub mod hci;
pub mod wifi;

const POLL_ROUNDS_PER_QUANTUM: usize = 64;
static CLOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn clock() -> std::sync::MutexGuard<'static, ()> {
    let guard = CLOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    MockDriver::get().reset();
    guard
}

pub struct Ready(AtomicBool);
impl Wake for Ready {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }
}
type Task<'a> = (Pin<Box<dyn Future<Output = ()> + 'a>>, Arc<Ready>, Waker);
pub struct Executor<'a> {
    tasks: Vec<Task<'a>>,
}
impl<'a> Executor<'a> {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }
    pub fn spawn(&mut self, future: impl Future<Output = ()> + 'a) {
        let ready = Arc::new(Ready(AtomicBool::new(true)));
        let waker = Waker::from(ready.clone());
        self.tasks.push((Box::pin(future), ready, waker));
    }
    pub fn task_lifetimes(&self) -> Vec<std::sync::Weak<Ready>> {
        self.tasks
            .iter()
            .map(|(_, ready, _)| Arc::downgrade(ready))
            .collect()
    }
    pub fn pump(&mut self) {
        for _ in 0..POLL_ROUNDS_PER_QUANTUM {
            let mut progress = false;
            for (future, ready, waker) in &mut self.tasks {
                if ready.0.swap(false, Ordering::AcqRel) {
                    progress = true;
                    assert!(
                        matches!(
                            future.as_mut().poll(&mut Context::from_waker(waker)),
                            Poll::Pending
                        ),
                        "firmware task exited unexpectedly"
                    );
                }
            }
            if !progress {
                return;
            }
        }
    }
    #[must_use]
    pub fn until(&mut self, limit_ms: u64, condition: impl Fn() -> bool) -> bool {
        for _ in 0..=limit_ms {
            self.pump();
            if condition() {
                return true;
            }
            MockDriver::get().advance(Duration::from_millis(1));
        }
        false
    }
    pub fn advance(&mut self, milliseconds: u64) {
        for _ in 0..milliseconds {
            self.pump();
            MockDriver::get().advance(Duration::from_millis(1));
        }
        self.pump();
    }
}

#[derive(Clone)]
pub struct Board(pub Rc<RefCell<Device>>);
#[derive(Clone, Copy)]
pub enum SerialWrite {
    Partial,
    Full,
    Zero,
    Oversized,
    Fault,
    Pending,
}
pub struct Device {
    pub trace: Vec<String>,
    pub uart_rx: VecDeque<u8>,
    pub uart_tx: Vec<u8>,
    pub buttons: [bool; 3],
    pub armed: [bool; 3],
    pub led: bool,
    pub buzzer: bool,
    pub divider: bool,
    pub divider_since: u64,
    pub selected: Option<Target>,
    pub panel_on: bool,
    pub busy_until: u64,
    pub busy_forever: bool,
    pub left: Vec<u8>,
    pub right: Vec<u8>,
    pub frames: Vec<Vec<u8>>,
    pub sleeping: Option<u32>,
    pub reject_sleep: bool,
    pub slots: [[u8; SLOT_BYTES]; 2],
    pub card: StorageStatus,
    pub rtc: [u8; 7],
    pub sensor_failure: Option<IoFailure>,
    pub sensor_crc_error: bool,
    pub rtc_failure: bool,
    pub charger_failure: bool,
    pub adc_pending: bool,
    pub adc_failure: bool,
    pub spi_failure: bool,
    pub flush_failure: bool,
    pub flush_pending: bool,
    pub serial_read_failure: bool,
    pub arm_failure: Option<usize>,
    pub storage_failure: bool,
    pub storage_fault_at: Option<usize>,
    pub storage_calls: usize,
    pub storage_fault_hits: usize,
    pub sleep_failure: bool,
    pub serial_write: SerialWrite,
    pub serial_writes: usize,
}
impl Board {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Device {
            trace: Vec::new(),
            uart_rx: VecDeque::new(),
            uart_tx: Vec::new(),
            buttons: [true; 3],
            armed: [false; 3],
            led: false,
            buzzer: false,
            divider: false,
            divider_since: 0,
            selected: None,
            panel_on: false,
            busy_until: 0,
            busy_forever: false,
            left: Vec::new(),
            right: Vec::new(),
            frames: Vec::new(),
            sleeping: None,
            reject_sleep: false,
            slots: [[0xff; SLOT_BYTES]; 2],
            card: StorageStatus::Absent,
            rtc: [0, 0x30, 0x19, 6, 5, 0x11, 0x26],
            sensor_failure: None,
            sensor_crc_error: false,
            rtc_failure: false,
            charger_failure: false,
            adc_pending: false,
            adc_failure: false,
            spi_failure: false,
            flush_failure: false,
            flush_pending: false,
            serial_read_failure: false,
            arm_failure: None,
            storage_failure: false,
            storage_fault_at: None,
            storage_calls: 0,
            storage_fault_hits: 0,
            sleep_failure: false,
            serial_write: SerialWrite::Partial,
            serial_writes: 0,
        })))
    }
    pub fn command(&self, command: &str) {
        self.0.borrow_mut().uart_rx.extend(command.bytes());
        self.0.borrow_mut().uart_rx.push_back(b'\n');
    }
    pub fn output(&self) -> String {
        String::from_utf8(self.0.borrow().uart_tx.clone()).unwrap()
    }
}
impl SlotStorage for Board {
    fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
        let mut device = self.0.borrow_mut();
        device.storage_calls += 1;
        if device.storage_failure || device.storage_fault_at == Some(device.storage_calls) {
            device.storage_fault_hits += 1;
            return Err(StorageError::Io);
        }
        *bytes = device.slots[slot];
        Ok(())
    }
    fn write(&mut self, slot: usize, offset: usize, bytes: &[u8]) -> Result<(), StorageError> {
        let mut device = self.0.borrow_mut();
        device.storage_calls += 1;
        if device.storage_failure || device.storage_fault_at == Some(device.storage_calls) {
            device.storage_fault_hits += 1;
            return Err(StorageError::Io);
        }
        device
            .trace
            .push(format!("flash {slot} {offset} {}", bytes.len()));
        device.slots[slot][offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        let mut device = self.0.borrow_mut();
        device.storage_calls += 1;
        if device.storage_failure || device.storage_fault_at == Some(device.storage_calls) {
            device.storage_fault_hits += 1;
            Err(StorageError::Io)
        } else {
            Ok(())
        }
    }
}
impl MeasurementIo for Board {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), IoFailure> {
        let mut device = self.0.borrow_mut();
        device
            .trace
            .push(format!("i2c-write {address:02x} {bytes:02x?}"));
        match address {
            0x44 => {
                assert_eq!(bytes, [0xfd]);
                device.sensor_failure.map_or(Ok(()), Err)
            }
            0x51 => {
                if device.rtc_failure {
                    return Err(IoFailure::Bus);
                }
                assert_eq!(bytes.len(), 8);
                assert_eq!(bytes[0], 2);
                device.rtc.copy_from_slice(&bytes[1..]);
                Ok(())
            }
            _ => panic!("undocumented I2C write {address:02x}"),
        }
    }
    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), IoFailure> {
        let device = self.0.borrow();
        assert_eq!(address, 0x44);
        if let Some(failure) = device.sensor_failure {
            return Err(failure);
        }
        let crc = e1004_bsp::sensors::sensor_crc([0x80, 0]);
        bytes.copy_from_slice(&[
            0x80,
            0,
            crc ^ u8::from(device.sensor_crc_error),
            0x80,
            0,
            crc,
        ]);
        Ok(())
    }
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> Result<(), IoFailure> {
        let device = self.0.borrow();
        match address {
            0x51 => {
                assert_eq!(write, [2]);
                if device.rtc_failure {
                    return Err(IoFailure::Bus);
                }
                read.copy_from_slice(&device.rtc);
            }
            0x6b => {
                assert_eq!(write, [8]);
                if device.charger_failure {
                    return Err(IoFailure::Bus);
                }
                read.copy_from_slice(&[0x10]);
            }
            _ => panic!("undocumented I2C transaction {address:02x}"),
        }
        Ok(())
    }
    fn battery_enable(&mut self, enabled: bool) {
        let mut device = self.0.borrow_mut();
        device.divider = enabled;
        device.divider_since = Instant::now().as_millis();
        device.trace.push(format!("divider {enabled}"));
    }
    fn battery_mv(&mut self) -> nb::Result<u16, IoFailure> {
        let device = self.0.borrow();
        assert!(device.divider);
        assert!(Instant::now().as_millis() - device.divider_since >= 10);
        if device.adc_failure {
            Err(nb::Error::Other(IoFailure::Bus))
        } else if device.adc_pending {
            Err(nb::Error::WouldBlock)
        } else {
            Ok(1950)
        }
    }
    fn extension_mv(&mut self) -> nb::Result<u16, IoFailure> {
        assert!(!self.0.borrow().divider);
        Ok(1234)
    }
}
impl ButtonsIo for Board {
    fn is_high(&mut self, index: usize) -> bool {
        self.0.borrow().buttons[index]
    }
    fn wake(&mut self, index: usize, enabled: bool) -> Result<(), IoFailure> {
        let mut device = self.0.borrow_mut();
        if enabled && device.arm_failure == Some(index) {
            return Err(IoFailure::Bus);
        }
        device.armed[index] = enabled;
        device.trace.push(format!("wake {index} {enabled}"));
        Ok(())
    }
}
impl ConsoleIo for Board {
    fn read(&mut self) -> nb::Result<u8, IoFailure> {
        if self.0.borrow().serial_read_failure {
            return Err(nb::Error::Other(IoFailure::Bus));
        }
        self.0
            .borrow_mut()
            .uart_rx
            .pop_front()
            .ok_or(nb::Error::WouldBlock)
    }
    fn write(&mut self, bytes: &[u8]) -> nb::Result<usize, IoFailure> {
        let mut device = self.0.borrow_mut();
        device.serial_writes += 1;
        let count = match device.serial_write {
            SerialWrite::Partial => bytes.len().min(3),
            SerialWrite::Full => bytes.len(),
            SerialWrite::Zero => {
                MockDriver::get().advance(Duration::from_millis(1));
                return Ok(0);
            }
            SerialWrite::Oversized => return Ok(bytes.len() + 1),
            SerialWrite::Fault => return Err(nb::Error::Other(IoFailure::Bus)),
            SerialWrite::Pending => return Err(nb::Error::WouldBlock),
        };
        device.uart_tx.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> nb::Result<(), IoFailure> {
        if self.0.borrow().flush_pending {
            Err(nb::Error::WouldBlock)
        } else if self.0.borrow().flush_failure {
            Err(nb::Error::Other(IoFailure::Bus))
        } else {
            Ok(())
        }
    }
    fn quiet(&mut self) {
        let mut device = self.0.borrow_mut();
        device.led = false;
        device.buzzer = false;
        device.trace.push("outputs quiet".into());
    }
    fn diagnose(&mut self, enabled: bool) {
        let mut device = self.0.borrow_mut();
        device.led = enabled;
        device.buzzer = enabled;
    }
}
impl PanelIo for Board {
    fn enable(&mut self, on: bool) -> Result<(), Error> {
        let mut device = self.0.borrow_mut();
        device.panel_on = on;
        device.trace.push(format!("panel power {on}"));
        Ok(())
    }
    fn reset(&mut self, high: bool) -> Result<(), Error> {
        self.0.borrow_mut().trace.push(format!("reset {high}"));
        Ok(())
    }
    fn command(&mut self, target: Target, command: u8, data: &[u8]) -> Result<(), Error> {
        let mut device = self.0.borrow_mut();
        assert!(device.panel_on);
        assert!(device.selected.is_none());
        device
            .trace
            .push(format!("panel {target:?} {command:02x} {data:02x?}"));
        if device.spi_failure {
            return Err(Error::Transport);
        }
        if command == 0x07 && device.sleep_failure {
            return Err(Error::Transport);
        }
        if command == 0x12 {
            assert_eq!(device.left.len(), FRAME_BYTES / 2);
            assert_eq!(device.right.len(), FRAME_BYTES / 2);
            let mut frame = Vec::with_capacity(FRAME_BYTES);
            for row in 0..usize::from(HEIGHT) {
                frame.extend_from_slice(&device.left[row * 300..(row + 1) * 300]);
                frame.extend_from_slice(&device.right[row * 300..(row + 1) * 300]);
            }
            device.frames.push(frame);
        }
        if [0x04, 0x12, 0x02].contains(&command) {
            device.busy_until = Instant::now().as_millis() + 5;
        }
        Ok(())
    }
    fn begin_data(&mut self, target: Target) -> Result<(), Error> {
        let mut device = self.0.borrow_mut();
        assert!(device.panel_on && device.selected.is_none());
        device.selected = Some(target);
        match target {
            Target::Left => device.left.clear(),
            Target::Right => device.right.clear(),
            Target::Both => panic!("frame data must target one controller"),
        }
        Ok(())
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut device = self.0.borrow_mut();
        if device.spi_failure {
            return Err(Error::Transport);
        }
        match device.selected {
            Some(Target::Left) => device.left.extend_from_slice(bytes),
            Some(Target::Right) => device.right.extend_from_slice(bytes),
            _ => panic!("unselected data transfer"),
        }
        Ok(())
    }
    fn end_data(&mut self) -> Result<(), Error> {
        let mut device = self.0.borrow_mut();
        assert!(device.selected.take().is_some());
        Ok(())
    }
    async fn delay_ms(&mut self, milliseconds: u32) {
        Timer::after_millis(u64::from(milliseconds)).await;
    }
    async fn wait_ready(&mut self, timeout_ms: u32) -> Result<(), Error> {
        let start = Instant::now().as_millis();
        loop {
            let now = Instant::now().as_millis();
            if now - start >= u64::from(timeout_ms) {
                return Err(Error::BusyTimeout);
            }
            {
                let device = self.0.borrow();
                if !device.busy_forever && now >= device.busy_until {
                    return Ok(());
                }
            }
            Timer::after_millis(1).await;
        }
    }
    async fn cooperate(&mut self) {
        embassy_futures::yield_now().await;
    }
}
impl BusIo for Board {
    fn stop(&mut self) {
        let mut device = self.0.borrow_mut();
        device.selected = None;
        device.panel_on = false;
        device.trace.push("panel stop".into());
    }
    fn sd_diagnose(&mut self) -> StorageStatus {
        self.0.borrow().card
    }
    fn deep_sleep(&mut self, seconds: u32) -> SleepOutcome {
        let mut device = self.0.borrow_mut();
        assert!(device.buttons.into_iter().all(|high| high));
        assert!(device.armed.into_iter().all(|armed| armed));
        assert!(
            !device.divider
                && !device.panel_on
                && device.selected.is_none()
                && !device.led
                && !device.buzzer
        );
        device.trace.push(format!("deep sleep {seconds}"));
        if device.reject_sleep {
            SleepOutcome::Rejected
        } else {
            device.sleeping = Some(seconds);
            SleepOutcome::Entered
        }
    }
}

pub fn loaded(board: &mut Board) -> (Settings, Option<Fault>) {
    device_runtime::tasks::boot_settings(board)
}
