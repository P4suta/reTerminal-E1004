---
name: reterminal-e1004
description: Develop the offline Rust firmware, shared screens, and native or Web previews for the Seeed reTerminal E1004.
---

# Device development

The device owns its app state and rendering.
Normal operation needs no network service or host computer.
The host and browser run the same Rust decision and drawing code.
Wi-Fi and Bluetooth Low Energy (BLE) are optional firmware features.

Read [hardware](references/hardware.md) before changing pins, panel transactions, sensors, storage, or power.
Read [development](references/development.md) for the crate boundaries, screen extension contract, toolchains, console, and preview events.
Read [verification](references/verification.md) when changing a contract or reporting verification.
The [source inventory](references/sources.toml) identifies source versions, licenses, dates, model scope, and unresolved physical assumptions.
Use the [preserved source catalog](references/source-catalog.md) for complete local manuals, figures, the schematic, and the panel source.
Read the saved edition before following a publisher URL.
Check the publisher when the task requires newer information or the saved edition doesn't cover the delivered hardware.
Keep saved source bytes intact and record corrections or interpretations in the authored references.

Keep the decision core dependency-free and `no_std`.
Convert external input into validated values before dispatching `AppEvent`.
Preserve missing, unavailable, stale, and failed measurements as distinct `Sample` variants.
Keep one owner of the shared panel and SD card Serial Peripheral Interface (SPI) bus.
Never replace a `BUSY` timeout with a successful refresh.
Never sleep while a display request or storage operation remains outstanding.

Define a behavioral contract and run its failing test before adding behavior.
Add the production function to the applicable source-bound proof harness and fault-injection tests.
Use `mise x -- just check` for host checks, `mise x -- just browser` for browser checks, and `mise x -- just firmware` for the four firmware configurations.
Run bounded heavy checks as described in the verification reference.
Keep a physical claim unverified until a device acceptance record supplies the measurement.
