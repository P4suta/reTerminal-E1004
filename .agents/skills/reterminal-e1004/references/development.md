# Development contracts

## Workspace

`device-core` contains geometry, validated measurements, settings encoding, and app transitions without dependencies or the standard library.
`device-ui` renders through `embedded-graphics` into a borrowed framebuffer and supplies the `Screen` extension trait.
`t133a01` owns the portable register protocol behind `PanelIo`.
`e1004-bsp` owns the pin inventory, sensor codecs, console parser, and transactional slot protocol.
`device-runtime` owns the production Embassy tasks behind peripheral ports.
`firmware` connects those tasks to ESP32-S3 effects through `esp-hal` 1.2.2 and `esp-rtos` 0.4.0.
`device-preview` shares event injection and replay between `native-preview` and `web-preview`.
`xtask` owns development commands and validation.

`mise.toml` and `rust-toolchain.toml` pin the host toolchain and command versions.
Dependency resolutions live in the root and firmware lockfiles.
Firmware has a separate Cargo workspace so host commands don't select Xtensa as their default target.
The Xtensa compiler is Espressif Rust 1.97.0.0 with the toolchain name `esp-e1004`.
The [Espressif toolchain instructions](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html) explain why ESP32-S3 needs the compiler fork.

Install the host tools and target before running the example.

```sh
mise install
mise x -- rustup target add wasm32-unknown-unknown
mise x -- just sim --png target/status.png --replay verification/fixtures/status.events
```

Install `espup` 0.19.0 and the pinned Xtensa components before the firmware build.

```sh
cargo install --locked espup --version 0.19.0
espup install --targets esp32s3 --toolchain-version 1.97.0.0 --crosstool-toolchain-version 16.1.0_20260609 --name esp-e1004
mise x -- just firmware
```

`xtask` derives compiler, GCC, and `LLVM` paths from `rustup` rather than storing machine paths in the repository.
The GCC and `LLVM` versions must match the versions selected in `xtask`.
Use `mise x -- just firmware offline`, `wifi`, `ble`, or `wifi,ble` for one configuration.
The default firmware command builds all four configurations.
Every build also generates a merged 32 MB flash image with espflash 4.7.0 and the actual partition table.
The settings sectors use custom partition type `0x40` with subtypes `0x00` and `0x01`.
Image generation checks the partition parser and bootloader packaging after linking the app.
The source binding gate parses the concrete hardware initialization and checks signal roles, peripheral tokens, pin numbers, and initial output levels.

## Screens and state

A screen receives an immutable `App` and draws into `Framebuffer` through `Screen::draw`.
`DeviceStatus`, `DiagnosticsScreen`, and `DrawingScreen` show the extension point.
Add a `ScreenId` variant, handle navigation exhaustively, and dispatch the screen in `device_ui::render` when adding a page.
Put external reads in an adapter and pass a `TelemetrySnapshot` through `AppEvent::Telemetry`.
Keep host clock and network reads outside screens.

`App::transition` returns the state and a fixed array of `Action` values.
An active update has an operation ID and start time.
Input changes the desired screen while one pending flag coalesces redraw requests.
Only the active operation's completion can finish an update.
Pending display work waits for a completed measurement, including a failed measurement result.
Refresh requests during measurement share that operation.
A timeout enters the failed state and refuses a late success notification.
Storage and measurement work must finish before sleep.
The default automatic refresh interval is 900 seconds.
The next periodic update retries a failed panel after the driver has stopped it.
The firmware retains a pending bus request when the bounded queue is full and keeps consuming app events.
Panel transfers yield after each controller row so input and measurement tasks can run during a refresh.
The runtime uses the shared `Presentation` type to keep the last successfully transferred snapshot.
An active generation owns display completion, cancellation, and panel-failure reporting.
Sleep tickets carry the current generation and interval and coalesce through an Embassy signal.
Before sleep, the bus task checks released buttons, stopped radio controllers, empty work queues, and completed measurement and storage work.
It flushes the console again after radio shutdown and rechecks admission after the asynchronous panel sleep operation.
Rejected sleep resumes input and output and retries through the same app transition.
The [task tests](../../../../crates/device-runtime/tests/e2e.rs) run these production futures with virtual time and protocol-aware ports.

## Preview

`mise x -- just sim` opens a desktop window with Left, Space, and Right as the three buttons.
The simulator also accepts `--png PATH`, `--raw PATH`, `--replay PATH`, and repeated `--event COMMAND` arguments.
`mise x -- just web` serves the browser preview at the printed loopback address.
The browser imports generated `wasm-bindgen` glue and runs event handling in Rust.
Normal preview operation needs no external script, image, font, or network API.

Preview commands use one event per line.
The status fixture is an executable example of clock, temperature, humidity, battery, charging, and SD input.
`previous`, `refresh`, and `next` dispatch buttons.
`wait`, `complete`, `timeout`, and `immediate` control display completion.
`tick MILLISECONDS` supplies monotonic time and `rotation 0` through `rotation 3` select orientation.
Measurement values accept `unknown`, `missing`, and `fault` as explicit states.
SD accepts `absent`, `ready`, or `fault`.
Wi-Fi and Bluetooth Low Energy accept `off`, `disconnected`, or `connected`.
Replay validates a candidate log before replacing the current preview state.
The browser accepts temperature in degrees Celsius and relative humidity from 0% to 100%, with two decimal places.
The shared `MeasurementInputs` boundary validates every field before one display request.
An invalid submission preserves the app, presented frame, and recording.
The `measurements` replay command stores that transaction as one event.
The recording and replay share one byte budget, so an accepted recording remains replayable.
Replay errors identify their source line.
The default browser example starts with explicitly simulated measurements.
Reset example restores the form and preview together.

The pure `Presentation` type retains the last successful frame and the active operation's snapshot.
Only a matching successful completion changes the presented frame.
Cancellation revokes the outstanding completion.
Preview fields are private, so adapters use events and read-only observations.
Completion mode controls when an operation completes and never selects a desired frame for drawing.

Native HTML buttons follow the [HTML button contract](https://html.spec.whatwg.org/multipage/form-elements.html#the-button-element).
Every field has an explicit label under the [W3C labeling guidance](https://www.w3.org/WAI/tutorials/forms/labels/).
The interface preserves visible [keyboard focus](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html) and [heading order](https://www.w3.org/WAI/tutorials/page-structure/headings/).
The screen-information region exposes state as text alongside the canvas.
The form applies [W3C validation guidance](https://www.w3.org/WAI/tutorials/forms/validation/) through exact Rust decimal parsing and validated measurement types.
Native date and number controls enforce their ranges and increments through the [HTML input contract](https://html.spec.whatwg.org/multipage/input.html#the-min-and-max-attributes).
The date control accepts device local time at minute or second precision.
Draft validation calls the same Rust validator as submission without changing device state or recording events.
Inline field errors preserve input and keyboard focus while editing.
An invalid submission focuses the field with the correction message.
Unrelated display and recording actions preserve field errors.
Completion uses native radio inputs with one selected mode under the [W3C radio group pattern](https://www.w3.org/WAI/ARIA/apg/patterns/radio/).
Action availability includes a visible explanation.
Controls and radio labels provide at least 44 by 44 CSS pixels under [Target Size](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html).
The black focus outline follows [Focus Appearance](https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html).
The focused checks cover the affected controls and don't establish complete accessibility conformance.
Errors name the field and correction, associate the message with the control, and focus the invalid control under [W3C notification guidance](https://www.w3.org/WAI/tutorials/forms/notifications/).
Submission uses the native form and submit-button contract, including Enter.
Controls avoid names that shadow methods under the [native form interface](https://html.spec.whatwg.org/multipage/forms.html#the-form-element).
Display feedback uses a separate [status message](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html) instead of announcing the entire diagnostic record after every action.
Optional controls use the native [details element](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-details-element).
Black and white, size, weight, spacing, and rules communicate hierarchy.

## Console and persistence

The USB connector uses a CH340K bridge to UART0 on GPIO43/44 at 115200 baud.
GPIO19/20 belong to onboard I2C.
`e1004_firmware::Extensions` reserves GPIO39/40 with `I2C1` and GPIO41/42 with `UART1` for app tasks.
The owned pin tokens also support digital input and output when the app selects that mode.
GPIO6 shares `ADC1` with the battery input and the measurement task owns both channels.
`ExtensionAdc::millivolts` returns the stored calibrated GPIO6 sample with freshness information.
Regular measurements and manual refresh update that sample.
`diagnose` prints the expansion sample, exercises the status light and buzzer, and writes and reads `E1004.TXT` on a present card.
The diagnostic file uses a reserved name and each diagnostic replaces its contents.
Use `status`, `diagnose`, `refresh`, `sleep`, `interval SECONDS`, and `rtc YYYY-MM-DDTHH:MM:SS` through the console.
Enter `help` for supported commands, argument formats, and interval limits.
The startup message advertises help, and rejected input reports a correction without echoing credentials.
Console guidance also follows the [WCAG2ICT text-interface guidance](https://www.w3.org/TR/wcag2ict-22/#text-command-line-terminal-applications-and-interfaces).
WCAG2ICT is informative guidance for software rather than a hardware conformance claim.
`resume last` restores the selected screen after sleep or restart and is the default.
`resume status` selects Status on every restart.
The status response includes the current policy and saved screen.
`wifi scan`, `wifi disconnect`, and `wifi connect SSID<TAB>PASSWORD` require the Wi-Fi feature.
`ble on` and `ble off` require the `ble` feature.
Supply credentials over the serial connection and keep them out of logs and replay fixtures.

Settings format 2 stores a format byte, generation, interval, orientation, resume policy, screen, `CRC-32` checksum, and commit marker in 24 bytes.
Format 1 remains readable and supplies the default policy and Status screen.
After quiescence, the runtime saves changed navigation once before sleep.
Unchanged timer wakes perform no settings writes.
An unsuccessful save reports `SettingsStorage` without changing the SD card status.
After a failed navigation save, the runtime waits for a changed value before another automatic attempt.
A restart can restore an earlier committed screen.
The adapter invalidates and syncs the inactive slot before writing the payload.
Boot reads both slots independently and retains a valid setting from either readable slot.
A failed read returns no usable bytes, even when the storage implementation changed its destination buffer before reporting the failure.
Boot reports the storage fault while preserving the other valid setting.
It syncs the payload before the commit marker, followed by a final sync.
Recovery validates both slots and selects the newer valid generation using serial-number arithmetic.
Generation comparisons assume fewer than 2³¹ successful writes between retained slots.
Two separate 4096-byte flash sectors isolate active and inactive records.
The partition table reserves both sectors before the factory app.
Basic screens continue without an SD card.
