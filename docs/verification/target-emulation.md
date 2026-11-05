# Target emulation

On November 6, 2026, the final-target emulation experiment used Espressif QEMU `esp-develop-9.2.2-20260417` for ESP32-S3.
The [publisher release](https://github.com/espressif/qemu/releases/tag/esp-develop-9.2.2-20260417) supplies the Apple silicon archive and checksum manifest.
The downloaded archive matched SHA-256 `bb8c15810565d3df1665dc34962430885e11bc95575b228fb44698146be1e9d6`.
The binary reported QEMU 9.2.2 with the expected Espressif release identifier.

The saved E1004 manual specifies 32 MB of flash and 8 MB of pseudo-static RAM.
`mise x -- just firmware` generates merged 32 MB flash images with the repository partition table through espflash 4.7.0.
QEMU rejected a complete image with `Drive size error: only 2, 4, 8, and 16MB images are supported`.
That rejection is a model-capacity failure rather than a successful firmware test.

A diagnostic image used the same Executable and Linkable Format (ELF) file and repository partition table with an 8 MB flash header.
The diagnostic image reached the vendor bootloader and app entry point.
The bootloader printed both settings sectors at `0x9000` and `0xa000` with custom type `0x40`, followed by the factory app at `0x10000`.
The substituted capacity makes this a diagnostic experiment rather than acceptance of the distributed image.
The CPU then remained in `esp_hal::soc::implementation::clocks::request_pll_clk` before the shared firmware tasks started.
For the inspected both-radio binary, the program counter was `0x4209ca5a` and the target address-to-line tool resolved that clock routine.
An earlier offline binary stopped in the same routine at `0x4202a96e`.
Direct binary loading also failed to supply the read-only memory services needed by the app.
The experiment stopped every emulator process after inspection.

The [Espressif feature matrix](https://github.com/espressif/esp-toolchain-docs/blob/03feaa6ff35a73e62f7553492c967815ead2cfb0/qemu/README.md) also lists general-purpose Serial Peripheral Interface (SPI), I2C, Wi-Fi, Bluetooth, and the general-purpose input/output matrix as unsupported.
Passing a CPU boot test would leave the board's control paths outside that emulator's model.
The [peripheral-port decision](../adr/execute-firmware-tasks-through-peripheral-ports.md) defines the supported alternative and its trust assumptions.
The production task tests execute delayed peripheral operations, input, persistence, display transfer, controller cleanup, and sleep admission.
The target compiler and hardware abstraction layer binding gate check their concrete target adapters.
Full execution of the distributed binary remains unsupported by this emulator release.
Actual current, color, calibration, RF behavior, and board timing require measurements from a delivered unit.
