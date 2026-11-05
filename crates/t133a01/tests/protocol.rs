use device_core::geometry::FRAME_BYTES;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
use t133a01::*;

#[derive(Debug, PartialEq, Eq)]
enum Trace {
    Enable(bool),
    Reset(bool),
    Command(Target, u8, Vec<u8>),
    Begin(Target),
    Data(Vec<u8>),
    End,
    Delay(u32),
    Ready(u32),
}

#[derive(Default)]
struct Bus {
    trace: Vec<Trace>,
    ready_fault: bool,
    data_fault: bool,
    cooperations: usize,
}

impl PanelIo for Bus {
    async fn cooperate(&mut self) {
        self.cooperations += 1;
    }
    fn enable(&mut self, on: bool) -> Result<(), Error> {
        self.trace.push(Trace::Enable(on));
        Ok(())
    }
    fn reset(&mut self, high: bool) -> Result<(), Error> {
        self.trace.push(Trace::Reset(high));
        Ok(())
    }
    fn command(&mut self, target: Target, cmd: u8, data: &[u8]) -> Result<(), Error> {
        self.trace.push(Trace::Command(target, cmd, data.to_vec()));
        Ok(())
    }
    fn begin_data(&mut self, target: Target) -> Result<(), Error> {
        self.trace.push(Trace::Begin(target));
        Ok(())
    }
    fn data(&mut self, data: &[u8]) -> Result<(), Error> {
        self.trace.push(Trace::Data(data.to_vec()));
        if self.data_fault {
            Err(Error::Transport)
        } else {
            Ok(())
        }
    }
    fn end_data(&mut self) -> Result<(), Error> {
        self.trace.push(Trace::End);
        Ok(())
    }
    async fn delay_ms(&mut self, ms: u32) {
        self.trace.push(Trace::Delay(ms));
    }
    async fn wait_ready(&mut self, ms: u32) -> Result<(), Error> {
        self.trace.push(Trace::Ready(ms));
        if self.ready_fault {
            Err(Error::BusyTimeout)
        } else {
            Ok(())
        }
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("trace adapters must be immediately ready"),
    }
}

#[test]
fn initialization_and_update_follow_the_pinned_vendor_protocol() {
    let mut panel = Panel::default();
    let mut bus = Bus::default();
    run(panel.initialize(&mut bus)).unwrap();
    assert_eq!(
        &bus.trace[..6],
        &[
            Trace::Enable(true),
            Trace::Reset(false),
            Trace::Delay(20),
            Trace::Reset(true),
            Trace::Delay(20),
            Trace::Ready(120_000)
        ]
    );
    let commands: Vec<_> = bus
        .trace
        .iter()
        .filter_map(|t| match t {
            Trace::Command(target, cmd, data) => Some((*target, *cmd, data.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        commands,
        vec![
            (
                Target::Left,
                0x74,
                vec![0, 12, 12, 217, 221, 221, 21, 21, 85]
            ),
            (Target::Both, 0xf0, vec![73, 85, 19, 93, 5, 16]),
            (Target::Both, 0, vec![223, 105]),
            (Target::Both, 0x30, vec![8]),
            (Target::Left, 0xa5, vec![68, 84, 0]),
            (Target::Both, 0x50, vec![55]),
            (Target::Both, 0x60, vec![3, 3]),
            (Target::Both, 0x86, vec![16]),
            (Target::Both, 0xe3, vec![34]),
            (Target::Both, 0x61, vec![4, 176, 3, 32]),
            (Target::Left, 1, vec![15, 0, 40, 44, 40, 56]),
            (Target::Left, 0xb6, vec![7]),
            (Target::Left, 6, vec![224, 32]),
            (Target::Left, 0xb7, vec![1]),
            (Target::Left, 5, vec![224, 32]),
            (Target::Left, 0xb0, vec![1]),
            (Target::Left, 0xb1, vec![2]),
        ]
    );
    bus.trace.clear();
    let mut frame = vec![0x11; FRAME_BYTES];
    for row in 0..1600 {
        frame[row * 600..row * 600 + 300].fill(0x00);
    }
    run(panel.display(&mut bus, &frame)).unwrap();
    assert_eq!(bus.cooperations, 3200);
    let mut right = false;
    let mut bytes = [0usize; 2];
    for trace in &bus.trace {
        match trace {
            Trace::Begin(Target::Right) => right = true,
            Trace::Data(data) => {
                assert!(data.iter().all(|b| *b == if right { 0x11 } else { 0x00 }));
                bytes[usize::from(right)] += data.len();
            }
            _ => {}
        }
    }
    assert_eq!(bytes, [480_000, 480_000]);
    let updates: Vec<_> = bus
        .trace
        .iter()
        .filter_map(|t| match t {
            Trace::Command(_, cmd, _) => Some(*cmd),
            _ => None,
        })
        .collect();
    assert_eq!(updates, [0xe0, 4, 0x12, 2]);
    assert_eq!(
        bus.trace
            .iter()
            .filter(|t| matches!(t, Trace::Ready(_)))
            .count(),
        4
    );
    bus.trace.clear();
    run(panel.sleep(&mut bus)).unwrap();
    assert!(bus
        .trace
        .contains(&Trace::Command(Target::Left, 7, vec![0xa5])));
    assert_eq!(bus.trace.last(), Some(&Trace::Enable(false)));
    assert_eq!(
        run(panel.display(&mut bus, &frame)),
        Err(Error::NotInitialized)
    );
}

#[test]
fn timeout_and_transport_errors_stop_the_protocol_and_release_the_bus() {
    let mut panel = Panel::default();
    let mut bus = Bus {
        ready_fault: true,
        ..Bus::default()
    };
    assert_eq!(run(panel.initialize(&mut bus)), Err(Error::BusyTimeout));
    assert!(!bus.trace.iter().any(|t| matches!(t, Trace::Command(..))));
    bus.ready_fault = false;
    run(panel.initialize(&mut bus)).unwrap();
    assert_eq!(
        run(panel.display(&mut bus, &[0; 3])),
        Err(Error::InvalidFrame)
    );
    bus.trace.clear();
    bus.data_fault = true;
    assert_eq!(
        run(panel.display(&mut bus, &vec![0x11; FRAME_BYTES])),
        Err(Error::Transport)
    );
    assert_eq!(bus.trace.last(), Some(&Trace::End));
    assert!(!bus.trace.contains(&Trace::Command(Target::Both, 4, vec![])));
}
