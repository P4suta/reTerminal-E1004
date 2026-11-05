use device_runtime::{
    console::{command, ConsoleIo},
    measurements::IoFailure,
    policy::Radios,
    state::System,
};
use embassy_time::{Duration, MockDriver};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

struct Serial {
    mode: u8,
    output: Vec<u8>,
}
impl ConsoleIo for Serial {
    fn read(&mut self) -> nb::Result<u8, IoFailure> {
        Err(nb::Error::WouldBlock)
    }
    fn write(&mut self, bytes: &[u8]) -> nb::Result<usize, IoFailure> {
        match self.mode {
            0 => {
                let count = bytes.len().min(3);
                self.output.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            1 => {
                MockDriver::get().advance(Duration::from_millis(1));
                Ok(0)
            }
            2 => Err(nb::Error::WouldBlock),
            _ => Err(nb::Error::Other(IoFailure::Bus)),
        }
    }
    fn flush(&mut self) -> nb::Result<(), IoFailure> {
        Ok(())
    }
    fn quiet(&mut self) {}
    fn diagnose(&mut self, _: bool) {}
}
pub fn check(data: &[u8]) {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    for line in text.lines().chain(core::iter::once(text)) {
        MockDriver::get().reset();
        let selector = data.first().copied().unwrap_or_default();
        let system = System::new(Radios {
            wifi: selector & 1 != 0,
            ble: selector & 2 != 0,
        });
        let mut serial = Serial {
            mode: selector % 4,
            output: Vec::new(),
        };
        let mut operation = Box::pin(command(&system, &mut serial, line));
        let mut accepted = None;
        for _ in 0..=1100 {
            if let Poll::Ready(value) = operation
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                accepted = Some(value);
                break;
            }
            MockDriver::get().advance(Duration::from_millis(1));
        }
        drop(operation);
        let accepted = accepted.expect("console command exceeded its virtual-time bound");
        if e1004_bsp::console::parse(line).is_err() {
            assert!(!accepted);
            assert!(system.events.is_empty() && system.bus.is_empty());
            assert!(!system.measure.signaled() && !system.rtc_set.signaled());
            assert!(system.wifi_command.is_empty() && !system.ble_enabled.signaled());
        }
        if !system.capabilities.wifi {
            assert!(system.wifi_command.is_empty());
        }
        if !system.capabilities.ble {
            assert!(!system.ble_enabled.signaled());
        }
        assert!(serial.output.len() <= 1024);
        if let Ok(e1004_bsp::console::Command::WifiConnect { .. }) = e1004_bsp::console::parse(line)
        {
            if system.capabilities.wifi {
                assert!(serial.output.is_empty());
            }
        }
    }
}
#[test]
fn committed_console_corpus_runs_through_the_production_dispatcher() {
    for entry in std::fs::read_dir("../../fuzz/corpus/boundaries").unwrap() {
        check(&std::fs::read(entry.unwrap().path()).unwrap());
    }
}
