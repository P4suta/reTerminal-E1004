# Hardware and protocol

## Model boundary

The [saved E1004 manual](upstream/wiki/e1004-manual.md) identifies an ESP32-S3 with 8 MB of pseudo-static RAM.
The device has a 1200 × 1600 Spectra 6 panel and a 5000 mAh battery.
The manual specifies 32 MB of flash storage.
The [saved E1004 schematic](upstream/hardware/e1004-schematic.pdf) carries the date January 5, 2026.
The source inventory records its revision.
The schematic prints the `CC BY-SA 4.0` license.
The [source catalog](source-catalog.md) records the saved originals, local figures, licenses, and publisher URLs.

The E1001 status-light pin and the E1003 SD power pin differ from the E1004 assignments.
E1003 GT911 controller and microphone examples also fall outside the E1004 board contract.
The shared cookbooks contain model-specific tabs and examples that don't all apply to E1004.
The E1004 expansion header supplies GPIO39/40 for I2C and GPIO41/42 for a universal asynchronous receiver-transmitter.
GPIO6 supports an analog-to-digital converter or digital I/O.
The Wiki calls the header J2, while the linked schematic labels the expansion connector J3.
Identify the connector by its signals before attaching hardware.

## Pin ownership

The [saved E1004 setup](upstream/seeed-gfx/Setup523_Seeed_reTerminal_E1004.h) assigns `SCK` to GPIO7, `MISO` to GPIO8, and `MOSI` to GPIO9.
The left and right controller selects use GPIO10 and GPIO2.
Panel `DC`, enable, `BUSY`, and reset use GPIO11, GPIO12, GPIO13, and GPIO38.
The panel enable signal is active low according to the schematic.
The implementation must use that polarity before device delivery.
Physical acceptance checks whether the delivered board matches the published circuit.

The [saved peripheral cookbook](upstream/wiki/arduino-peripherals.md) assigns the three buttons to GPIO3/4/5, the E1004 status light to GPIO48, and the buzzer to GPIO45.
The status light is active low.
The battery divider uses GPIO1 with GPIO21 as its enable and a factor of 2 between converter voltage and battery voltage.
Wait at least 10 ms after enabling the divider.
Calibrated converter results still need comparison with a meter on the delivered board.

The SD card uses GPIO14 for chip select, GPIO15 for active-low card detection, and GPIO16 for power.
The panel and card share the same Serial Peripheral Interface (SPI) pins.
The firmware bus task owns those resources and deselects every device before selecting another.
The board adapter's reserved-pin inventory rejects duplicate assignments at compile time.
The `esp-hal` peripheral tokens prevent another adapter from taking an already moved pin.

## Panel contract

The [saved T133A01 source](upstream/seeed-gfx/T133A01_Defines.h) supplies the register sequence and controller split.
E1004's setup overrides the generic header dimensions to 1200 × 1600.
The canonical framebuffer has 960000 bytes, 2 pixels per byte, and the earlier pixel in the high nibble.
Wire colors are black 0, white 1, yellow 2, red 3, blue 5, and green 6.
The renderer uses those wire codes directly.
Each controller receives 300 bytes from every row, for 480000 bytes per controller.
Rotation changes logical coordinates before packing and leaves the canonical buffer layout unchanged.

Reset stays low for 20 ms and high for 20 ms before waiting for `BUSY` to become high.
The driver programs both controllers where the pinned source selects both chip selects.
The source's unbounded `BUSY` loops become explicit 120000 ms errors.
Transfers finish before power-on, refresh, and power-off commands.
Each phase waits for ready and keeps the source's 30 ms inter-command delay.
Sleep uses command `0x07` with `0xA5`, then removes panel power.
Wake runs initialization again.
SPI failures deselect the device and preserve the primary transport error.

## Measurements and power

The onboard I2C bus uses GPIO19/20.
SHT40 uses address `0x44` and PCF8563 uses `0x51`.
The [Sensirion datasheet](https://sensirion.com/media/documents/33FD6951/662A593A/HT_DS_Datasheet_SHT4x.pdf) defines the conversion and cyclic redundancy check.
The polynomial is `0x31`, the initial checksum is `0xFF`, and the measurement command is `0xFD`.
Temperature uses signed hundredths of a degree Celsius.
Humidity uses 0.01 % units and clips to the datasheet's 0–100 % output interval.

The [PCF8563 datasheet](https://www.nxp.com/docs/en/data-sheet/PCF8563.pdf) defines the voltage-low flag and binary-coded decimal registers.
The [saved real-time clock cookbook](upstream/wiki/arduino-rtc.md) selects 2000–2099 with a clear century bit.
The adapter rejects voltage loss, invalid binary-coded decimal digits, impossible dates, and dates outside that century.
It preserves an existing clock at boot.

The schematic identifies the charger as SY6974B.
The [Silergy register description](https://www.silergy.com/download/downloadFile?ftype=note&id=4013&type=product) assigns address `0x6B` and charging state to register `0x08` bits 4:3.
The firmware reads charging status without modifying charging limits or ship-mode settings.
A failed charger read yields an unknown status.
The voltage-based battery percentage is a piecewise estimate rather than a fuel-gauge measurement.
Tests and proofs check the estimate's monotonicity and bounds.
Electrochemical accuracy requires physical measurements.

## Physical acceptance

Published pin assignments, register values, signal polarity, and control sequences are software requirements before device delivery.
Missing host tests for startup, cancellation, wake configuration, or power sequencing remain software verification work.
Treat those gaps as software work rather than physical measurements.
Model those interfaces and exercise the production control paths before the device arrives.

Record the delivered board revision, firmware digest, supply, instrument, and ambient temperature before acceptance measurements.
Capture all six colors, controller seams, every rotation, complete refresh duration, timeout recovery, and a power cycle after deep sleep.
Check each button while awake and as a wake source, timer wake, clock backup loss, card removal during writes, charge status, serial output, and optional radio operation.
Measure sleep current with panel, SD power, divider, status light, and buzzer inactive.
Record current and elapsed time with units rather than estimating battery life from compiler or host results.
The project has no physical acceptance measurements.
