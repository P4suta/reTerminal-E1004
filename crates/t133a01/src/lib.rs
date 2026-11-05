#![no_std]

use device_core::{
    app::DISPLAY_TIMEOUT_MS,
    geometry::{controller_source, CONTROLLER_ROW_BYTES, FRAME_BYTES, HEIGHT},
};

const READY_TIMEOUT_MS: u32 = DISPLAY_TIMEOUT_MS as u32;
const RESET_DELAY_MS: u32 = 20;
const TRANSFER_SETTLE_MS: u32 = 10;
const PHASE_SETTLE_MS: u32 = 30;
const SLEEP_SETTLE_MS: u32 = 1;
const PREPARE_DISPLAY: (u8, &[u8]) = (0xe0, &[1]);
const DISPLAY_PHASES: &[(u8, &[u8])] = &[(4, &[]), (0x12, &[0]), (2, &[0])];
const DEEP_SLEEP: (u8, &[u8]) = (7, &[0xa5]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Transport,
    BusyTimeout,
    InvalidFrame,
    NotInitialized,
}

pub trait PanelIo {
    fn cooperate(&mut self) -> impl core::future::Future<Output = ()>;
    fn enable(&mut self, on: bool) -> Result<(), Error>;
    fn reset(&mut self, high: bool) -> Result<(), Error>;
    fn command(&mut self, target: Target, command: u8, data: &[u8]) -> Result<(), Error>;
    fn begin_data(&mut self, target: Target) -> Result<(), Error>;
    fn data(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn end_data(&mut self) -> Result<(), Error>;
    fn delay_ms(&mut self, milliseconds: u32) -> impl core::future::Future<Output = ()>;
    fn wait_ready(
        &mut self,
        timeout_ms: u32,
    ) -> impl core::future::Future<Output = Result<(), Error>>;
}

#[derive(Default)]
pub struct Panel {
    initialized: bool,
}

impl Panel {
    pub async fn initialize(&mut self, io: &mut impl PanelIo) -> Result<(), Error> {
        self.initialized = false;
        io.enable(true)?;
        io.reset(false)?;
        io.delay_ms(RESET_DELAY_MS).await;
        io.reset(true)?;
        io.delay_ms(RESET_DELAY_MS).await;
        io.wait_ready(READY_TIMEOUT_MS).await?;
        for (target, command, data) in INIT {
            io.command(*target, *command, data)?;
        }
        self.initialized = true;
        Ok(())
    }
    pub async fn display(&mut self, io: &mut impl PanelIo, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() != FRAME_BYTES {
            return Err(Error::InvalidFrame);
        }
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        self.initialized = false;
        io.command(Target::Both, PREPARE_DISPLAY.0, PREPARE_DISPLAY.1)?;
        io.wait_ready(READY_TIMEOUT_MS).await?;
        io.delay_ms(TRANSFER_SETTLE_MS).await;
        for (right, target) in [(false, Target::Left), (true, Target::Right)] {
            io.begin_data(target)?;
            let result = async {
                for row in 0..HEIGHT {
                    let start = controller_source(right, row, 0).ok_or(Error::InvalidFrame)?;
                    io.data(&bytes[start..start + usize::from(CONTROLLER_ROW_BYTES)])?;
                    io.cooperate().await;
                }
                Ok(())
            }
            .await;
            let cleanup = io.end_data();
            result.and(cleanup)?;
        }
        for (command, data) in DISPLAY_PHASES {
            io.command(Target::Both, *command, data)?;
            io.wait_ready(READY_TIMEOUT_MS).await?;
            io.delay_ms(PHASE_SETTLE_MS).await;
        }
        self.initialized = true;
        Ok(())
    }
    pub async fn sleep(&mut self, io: &mut impl PanelIo) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        self.initialized = false;
        io.command(Target::Left, DEEP_SLEEP.0, DEEP_SLEEP.1)?;
        io.delay_ms(SLEEP_SETTLE_MS).await;
        io.wait_ready(READY_TIMEOUT_MS).await?;
        io.enable(false)
    }
}

const INIT: &[(Target, u8, &[u8])] = &[
    (
        Target::Left,
        0x74,
        &[0x00, 0x0c, 0x0c, 0xd9, 0xdd, 0xdd, 0x15, 0x15, 0x55],
    ),
    (Target::Both, 0xf0, &[0x49, 0x55, 0x13, 0x5d, 0x05, 0x10]),
    (Target::Both, 0x00, &[0xdf, 0x69]),
    (Target::Both, 0x30, &[0x08]),
    (Target::Left, 0xa5, &[0x44, 0x54, 0x00]),
    (Target::Both, 0x50, &[0x37]),
    (Target::Both, 0x60, &[0x03, 0x03]),
    (Target::Both, 0x86, &[0x10]),
    (Target::Both, 0xe3, &[0x22]),
    (Target::Both, 0x61, &[0x04, 0xb0, 0x03, 0x20]),
    (Target::Left, 0x01, &[0x0f, 0x00, 0x28, 0x2c, 0x28, 0x38]),
    (Target::Left, 0xb6, &[0x07]),
    (Target::Left, 0x06, &[0xe0, 0x20]),
    (Target::Left, 0xb7, &[0x01]),
    (Target::Left, 0x05, &[0xe0, 0x20]),
    (Target::Left, 0xb0, &[0x01]),
    (Target::Left, 0xb1, &[0x02]),
];
