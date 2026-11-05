use bt_hci_transport::{PacketKind, PacketToController, PacketToHost, Transport};
use device_runtime::{ble::BleFactory, measurements::IoFailure};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    task::{Poll, Waker},
};

#[derive(Debug, Clone, Copy)]
pub struct HciError;
impl core::fmt::Display for HciError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("injected HCI failure")
    }
}
impl core::error::Error for HciError {}
impl embedded_io::Error for HciError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}
impl From<bt_hci::FromHciBytesError> for HciError {
    fn from(_: bt_hci::FromHciBytesError) -> Self {
        Self
    }
}
impl From<bt_hci_transport::ReadHciError<core::convert::Infallible>> for HciError {
    fn from(_: bt_hci_transport::ReadHciError<core::convert::Infallible>) -> Self {
        Self
    }
}

#[derive(Default)]
pub struct Controller {
    queue: VecDeque<(PacketKind, Vec<u8>)>,
    reader: Option<Waker>,
    pub commands: Vec<Vec<u8>>,
    pub acl: Vec<Vec<u8>>,
    pub advertising: bool,
    pub opens: usize,
    pub closes: usize,
    pub fail: bool,
}
#[derive(Clone, Default)]
pub struct Hci(pub Rc<RefCell<Controller>>);
impl Hci {
    pub fn receive(&self, kind: PacketKind, bytes: Vec<u8>) {
        let mut model = self.0.borrow_mut();
        model.queue.push_back((kind, bytes));
        if let Some(waker) = model.reader.take() {
            waker.wake();
        }
    }
    pub fn connect(&self) {
        self.0.borrow_mut().advertising = false;
        self.receive(
            PacketKind::Event,
            vec![
                0x3e, 19, 1, 0, 1, 0, 1, 0, 6, 5, 4, 3, 2, 1, 24, 0, 0, 0, 200, 0, 0,
            ],
        );
    }
    pub fn disconnect(&self) {
        self.receive(PacketKind::Event, vec![5, 4, 0, 1, 0, 0x13]);
    }
    pub fn att(&self, bytes: &[u8]) {
        let mut packet = vec![
            1,
            0x20,
            (bytes.len() + 4) as u8,
            0,
            bytes.len() as u8,
            0,
            4,
            0,
        ];
        packet.extend_from_slice(bytes);
        self.receive(PacketKind::AclData, packet);
    }
}
impl embedded_io::ErrorType for Hci {
    type Error = HciError;
}
struct Bytes(Vec<u8>);
impl embedded_io::ErrorType for Bytes {
    type Error = HciError;
}
impl embedded_io::Write for Bytes {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, HciError> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> Result<(), HciError> {
        Ok(())
    }
}
impl Transport for Hci {
    async fn read<'a, P: PacketToHost<'a>>(&self, rx: &'a mut [u8]) -> Result<P, HciError> {
        let (kind, bytes) = core::future::poll_fn(|context| {
            let mut model = self.0.borrow_mut();
            if model.fail {
                return Poll::Ready(Err(HciError));
            }
            match model.queue.pop_front() {
                Some(packet) => Poll::Ready(Ok(packet)),
                None => {
                    model.reader = Some(context.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await?;
        Ok(
            P::read_hci(kind, &mut bytes.as_slice(), rx)
                .expect("invalid modeled controller packet"),
        )
    }
    async fn write<P: PacketToController>(&self, packet: &P) -> Result<(), HciError> {
        if self.0.borrow().fail {
            return Err(HciError);
        }
        let mut writer = Bytes(Vec::new());
        packet.write_hci(&mut writer)?;
        let bytes = writer.0;
        match P::KIND {
            PacketKind::Cmd => {
                let opcode = u16::from_le_bytes([bytes[0], bytes[1]]);
                #[cfg(miri)]
                eprintln!("hci-stage=command opcode={opcode:04x}");
                let returned: &[u8] = match opcode {
                    0x0c03 | 0x0c01 | 0x0c63 | 0x2001 | 0x2005 | 0x2006 | 0x2008 | 0x2009
                    | 0x200a | 0x0c33 => &[],
                    0x200f => &[8],
                    0x2002 => &[251, 0, 4],
                    0x1009 => &[1, 2, 3, 4, 5, 6],
                    _ => panic!("unmodeled HCI command {opcode:04x}: {bytes:02x?}"),
                };
                if opcode == 0x200a {
                    self.0.borrow_mut().advertising = bytes[3] != 0;
                }
                let mut event = vec![0x0e, (returned.len() + 4) as u8, 1, bytes[0], bytes[1], 0];
                event.extend_from_slice(returned);
                self.0.borrow_mut().commands.push(bytes);
                self.receive(PacketKind::Event, event);
            }
            PacketKind::AclData => {
                self.0.borrow_mut().acl.push(bytes);
                self.receive(PacketKind::Event, vec![0x13, 5, 1, 1, 0, 1, 0]);
            }
            _ => panic!("unexpected HCI packet kind"),
        }
        Ok(())
    }
}
pub struct Factory(pub Hci);
pub struct Lease(Hci);
impl Drop for Lease {
    fn drop(&mut self) {
        let mut controller = self.0 .0.borrow_mut();
        controller.closes += 1;
        controller.advertising = false;
    }
}
impl embedded_io::ErrorType for Lease {
    type Error = HciError;
}
impl Transport for Lease {
    async fn read<'a, P: PacketToHost<'a>>(&self, rx: &'a mut [u8]) -> Result<P, HciError> {
        self.0.read(rx).await
    }
    async fn write<P: PacketToController>(&self, packet: &P) -> Result<(), HciError> {
        self.0.write(packet).await
    }
}
impl BleFactory for Factory {
    type Controller<'a> = bt_hci::controller::ExternalController<Lease, 20>;
    fn start(&mut self) -> Result<(Self::Controller<'_>, [u8; 6]), IoFailure> {
        #[cfg(miri)]
        eprintln!("hci-stage=controller-start");
        self.0 .0.borrow_mut().opens += 1;
        Ok((
            bt_hci::controller::ExternalController::new(Lease(self.0.clone())),
            [1, 2, 3, 4, 5, 6],
        ))
    }
}
