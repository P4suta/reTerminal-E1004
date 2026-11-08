#![no_std]
#![no_main]
#![feature(allocator_api)]
use device_core::telemetry::WakeReason;

extern crate alloc;

mod panel;
mod peripherals;
mod radio;
mod storage;

use alloc::vec::Vec;
use device_core::{
    app::*,
    geometry::{FRAME_BYTES, PACKED_WHITE},
};
use device_runtime::{
    policy::Radios,
    state::{LogRecord, System},
};
use embassy_executor::Spawner;
use esp_hal::{
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    spi::master::{Config as SpiConfig, Spi},
    time::Rate,
    timer::timg::TimerGroup,
    uart::{Config as UartConfig, Uart},
};
esp_bootloader_esp_idf::esp_app_desc!();

const INTERNAL_HEAP_BYTES: usize = 96 * 1024;
const SPI_INITIAL_KHZ: u32 = 400;
const I2C_KHZ: u32 = 100;
const CONSOLE_BAUD: u32 = 115200;
static SYSTEM: System = System::new(Radios {
    wifi: cfg!(feature = "wifi"),
    ble: cfg!(feature = "ble"),
});

fn record(record: LogRecord) {
    SYSTEM.record(record);
}
fn snapshot() -> Option<App> {
    SYSTEM.snapshot()
}
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let p = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: INTERNAL_HEAP_BYTES);
    esp_alloc::psram_allocator!(
        p.PSRAM,
        esp_hal::psram,
        esp_hal::psram::PsramConfig {
            mode: esp_hal::psram::PsramMode::OctalSpi,
            ..Default::default()
        }
    );
    let timer = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timer.timer0, p.FROM_CPU_INTR0);
    let spi = Spi::new(
        p.SPI2,
        SpiConfig::default().with_frequency(Rate::from_khz(SPI_INITIAL_KHZ)),
    )
    .unwrap()
    .with_sck(p.GPIO7)
    .with_mosi(p.GPIO9)
    .with_miso(p.GPIO8);
    let hardware = panel::Hardware {
        spi,
        left: Output::new(p.GPIO10, Level::High, OutputConfig::default()),
        right: Output::new(p.GPIO2, Level::High, OutputConfig::default()),
        dc: Output::new(p.GPIO11, Level::High, OutputConfig::default()),
        reset: Output::new(p.GPIO38, Level::High, OutputConfig::default()),
        enable: Output::new(p.GPIO12, Level::High, OutputConfig::default()),
        busy: Input::new(p.GPIO13, InputConfig::default()),
        sd_cs: Output::new(p.GPIO14, Level::High, OutputConfig::default()),
        sd_enable: Output::new(p.GPIO16, Level::Low, OutputConfig::default()),
        sd_detect: Input::new(p.GPIO15, InputConfig::default()),
        low_power: esp_hal::rtc_cntl::sleep::LowPower::new(p.LPWR),
    };
    let i2c = I2c::new(
        p.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(I2C_KHZ)),
    )
    .unwrap()
    .with_sda(p.GPIO19)
    .with_scl(p.GPIO20);
    let uart = Uart::new(p.UART0, UartConfig::default().with_baudrate(CONSOLE_BAUD))
        .unwrap()
        .with_tx(p.GPIO43)
        .with_rx(p.GPIO44);
    let buttons = [
        Input::new(p.GPIO3, InputConfig::default()),
        Input::new(p.GPIO4, InputConfig::default()),
        Input::new(p.GPIO5, InputConfig::default()),
    ];
    let adc = p.ADC1;
    let battery_pin = p.GPIO1;
    let battery_enable = Output::new(p.GPIO21, Level::Low, OutputConfig::default());
    let led = Output::new(p.GPIO48, Level::High, OutputConfig::default());
    let buzzer = Output::new(p.GPIO45, Level::Low, OutputConfig::default());
    let _extension = e1004_firmware::Extensions {
        sda: p.GPIO39,
        scl: p.GPIO40,
        rx: p.GPIO41,
        tx: p.GPIO42,
        analog: e1004_firmware::ExtensionAdc,
        i2c: p.I2C1,
        uart: p.UART1,
    };
    let mut flash = storage::FlashSlots::new(esp_storage::FlashStorage::new(p.FLASH));
    record(LogRecord::Event("boot"));
    let (settings, boot_error) = device_runtime::tasks::boot_settings(&mut flash);
    if let Some(fault) = boot_error {
        record(LogRecord::Fault(fault));
    }
    spawner.spawn(bus_task(hardware, flash).unwrap());
    spawner.spawn(peripherals::buttons(buttons).unwrap());
    spawner
        .spawn(peripherals::measurements(i2c, adc, battery_pin, p.GPIO6, battery_enable).unwrap());
    spawner.spawn(peripherals::console(uart, led, buzzer).unwrap());
    spawner.spawn(ticks().unwrap());
    radio::start(
        spawner,
        #[cfg(feature = "wifi")]
        p.WIFI,
        #[cfg(feature = "ble")]
        p.BT,
    )
    .await;
    let reason = esp_hal::rtc_cntl::wakeup_cause();
    let wake = if reason.contains(esp_hal::rtc_cntl::WakeupSource::Timer) {
        WakeReason::Timer
    } else if reason.contains(esp_hal::rtc_cntl::WakeupSource::Ext1) {
        WakeReason::Button
    } else {
        WakeReason::PowerOn
    };
    device_runtime::tasks::application_loop(&SYSTEM, settings, boot_error, wake, || {
        (
            esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into()) as u32,
            esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::External.into()) as u32,
        )
    })
    .await;
}

#[embassy_executor::task]
async fn ticks() {
    device_runtime::tasks::ticks(&SYSTEM).await;
}

#[embassy_executor::task]
async fn bus_task(hardware: panel::Hardware, flash: storage::FlashSlots) {
    let mut frame = Vec::<u8, esp_alloc::ExternalMemory>::new_in(esp_alloc::ExternalMemory);
    if frame.try_reserve_exact(FRAME_BYTES).is_ok() {
        frame.resize(FRAME_BYTES, PACKED_WHITE);
    } else {
        record(LogRecord::Event("framebuffer_allocation_failed"));
    }
    let mut image = Vec::<u8, esp_alloc::ExternalMemory>::new_in(esp_alloc::ExternalMemory);
    if image.try_reserve_exact(FRAME_BYTES).is_ok() {
        image.resize(FRAME_BYTES, PACKED_WHITE);
    }
    device_runtime::tasks::bus_loop_with_media(&SYSTEM, hardware, flash, &mut frame, &mut image)
        .await;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
