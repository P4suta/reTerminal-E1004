use device_core::geometry::FRAME_BYTES;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
use t133a01::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Enable,
    Reset,
    Command,
    Begin,
    Data,
    End,
    Ready,
}
#[derive(Default)]
struct Bus {
    operations: Vec<Operation>,
    fail: Option<usize>,
    selected: bool,
    cleanup_fault: bool,
}
impl Bus {
    fn operation(&mut self, operation: Operation) -> Result<(), Error> {
        self.operations.push(operation);
        if self.fail == Some(self.operations.len()) {
            Err(Error::Transport)
        } else {
            Ok(())
        }
    }
}
impl PanelIo for Bus {
    async fn cooperate(&mut self) {}
    fn enable(&mut self, _: bool) -> Result<(), Error> {
        self.operation(Operation::Enable)
    }
    fn reset(&mut self, _: bool) -> Result<(), Error> {
        self.operation(Operation::Reset)
    }
    fn command(&mut self, _: Target, _: u8, _: &[u8]) -> Result<(), Error> {
        self.operation(Operation::Command)
    }
    fn begin_data(&mut self, _: Target) -> Result<(), Error> {
        self.operation(Operation::Begin)?;
        assert!(!self.selected);
        self.selected = true;
        Ok(())
    }
    fn data(&mut self, _: &[u8]) -> Result<(), Error> {
        assert!(self.selected);
        self.operation(Operation::Data)
    }
    fn end_data(&mut self) -> Result<(), Error> {
        assert!(self.selected);
        self.selected = false;
        self.operation(Operation::End)?;
        if self.cleanup_fault {
            Err(Error::BusyTimeout)
        } else {
            Ok(())
        }
    }
    async fn delay_ms(&mut self, _: u32) {}
    async fn wait_ready(&mut self, _: u32) -> Result<(), Error> {
        self.operation(Operation::Ready)
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test transport must be ready"),
    }
}
#[test]
fn every_initialization_transfer_and_sleep_failure_is_terminal_until_reinitialization() {
    let frame = vec![0x11; FRAME_BYTES];
    let mut injected = 0;
    for phase in 0..3 {
        let mut baseline = Bus::default();
        let mut panel = Panel::default();
        if phase != 0 {
            run(panel.initialize(&mut baseline)).unwrap();
            baseline.operations.clear();
        }
        match phase {
            0 => run(panel.initialize(&mut baseline)).unwrap(),
            1 => run(panel.display(&mut baseline, &frame)).unwrap(),
            _ => run(panel.sleep(&mut baseline)).unwrap(),
        }
        assert_eq!(
            baseline.operations.len(),
            match phase {
                0 => 21,
                1 => 3212,
                _ => 3,
            }
        );
        for (index, operation) in baseline.operations.iter().enumerate() {
            injected += 1;
            let mut bus = Bus::default();
            let mut panel = Panel::default();
            if phase != 0 {
                run(panel.initialize(&mut bus)).unwrap();
                bus.operations.clear();
            }
            bus.fail = Some(index + 1);
            let result = match phase {
                0 => run(panel.initialize(&mut bus)),
                1 => run(panel.display(&mut bus, &frame)),
                _ => run(panel.sleep(&mut bus)),
            };
            assert_eq!(
                result,
                Err(Error::Transport),
                "phase={phase} operation={index}"
            );
            assert!(!bus.selected);
            let expected_calls = index + 1 + usize::from(*operation == Operation::Data);
            assert_eq!(
                bus.operations.len(),
                expected_calls,
                "phase={phase} operation={index}"
            );
            assert_eq!(
                run(panel.display(&mut bus, &frame)),
                Err(Error::NotInitialized)
            );
            bus.fail = None;
            run(panel.initialize(&mut bus)).unwrap();
        }
    }
    assert_eq!(injected, 3236);
    println!("Injected transport failures at all {injected} fallible panel operations");
}

#[test]
fn a_cleanup_failure_preserves_the_primary_transfer_error() {
    let mut bus = Bus::default();
    let mut panel = Panel::default();
    run(panel.initialize(&mut bus)).unwrap();
    bus.operations.clear();
    bus.fail = Some(4);
    bus.cleanup_fault = true;
    assert_eq!(
        run(panel.display(&mut bus, &vec![0x11; FRAME_BYTES])),
        Err(Error::Transport)
    );
    assert_eq!(bus.operations.last(), Some(&Operation::End));
    assert!(!bus.selected);
}
