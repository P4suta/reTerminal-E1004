use device_runtime::{
    measurements::IoFailure,
    wifi::{Network, WifiFactory, WifiSession, MAX_NETWORKS},
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct Controller {
    pub scan_delay_ms: u64,
    pub opened: usize,
    pub closed: usize,
    pub fail_start: bool,
    pub fail_operation: bool,
    pub stall_scan: bool,
    pub stall_connect: bool,
    pub stall_disconnect: bool,
    pub scans: usize,
    pub connects: usize,
    pub disconnects: usize,
}
#[derive(Clone, Default)]
pub struct Factory(pub Rc<RefCell<Controller>>);
pub struct Session(Factory);
impl Drop for Session {
    fn drop(&mut self) {
        self.0 .0.borrow_mut().closed += 1;
    }
}
impl WifiFactory for Factory {
    type Session<'a> = Session;
    fn start(&mut self) -> Result<Session, IoFailure> {
        if self.0.borrow().fail_start {
            return Err(IoFailure::Bus);
        }
        self.0.borrow_mut().opened += 1;
        Ok(Session(self.clone()))
    }
}
impl WifiSession for Session {
    async fn scan(&mut self) -> Result<heapless::Vec<Network, MAX_NETWORKS>, IoFailure> {
        self.0 .0.borrow_mut().scans += 1;
        let delay = self.0 .0.borrow().scan_delay_ms;
        embassy_time::Timer::after_millis(delay).await;
        if self.0 .0.borrow().stall_scan {
            core::future::pending::<()>().await;
        }
        if self.0 .0.borrow().fail_operation {
            return Err(IoFailure::Bus);
        }
        Ok([Network {
            ssid: "fixture".try_into().unwrap(),
            signal_dbm: -42,
        }]
        .into_iter()
        .collect())
    }
    async fn connect(&mut self, ssid: &str, password: &str) -> Result<(), IoFailure> {
        assert_eq!(ssid, "fixture");
        assert!(password == "private-key" || password.is_empty());
        self.0 .0.borrow_mut().connects += 1;
        if self.0 .0.borrow().stall_connect {
            core::future::pending::<()>().await;
        }
        if self.0 .0.borrow().fail_operation {
            Err(IoFailure::Bus)
        } else {
            Ok(())
        }
    }
    async fn disconnect(&mut self) -> Result<(), IoFailure> {
        self.0 .0.borrow_mut().disconnects += 1;
        if self.0 .0.borrow().stall_disconnect {
            core::future::pending::<()>().await;
        }
        if self.0 .0.borrow().fail_operation {
            Err(IoFailure::Bus)
        } else {
            Ok(())
        }
    }
}
