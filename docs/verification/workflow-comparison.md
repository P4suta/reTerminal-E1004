# Documented factory and offline Rust workflows

The comparison covers documented prerequisites and executed software tasks after firmware installation.
The [preserved factory manual](../../.agents/skills/reterminal-e1004/references/upstream/wiki/e1004-manual.md) describes the SenseCraft Seeedash workflow.
The Rust firmware requires a separate build and initial flash, while Seeed preinstalls the factory firmware.
The project records that installation cost rather than treating both starting points as identical.
User preference, physical interaction quality, and real display latency require separate observations.

## View a local status screen

The factory manual's custom-dashboard workflow configures a 2.4 GHz network, signs in to SenseCraft, pairs a device, and deploys a dashboard or template.
Its alternative app workflow also uses a SenseCraft account and network configuration.
The manual also describes product and network instructions shown before pairing.
The Rust firmware renders its built-in Device Status screen from local real-time clock, sensor, charger, and battery inputs at boot.
Production-task tests exercise that screen with radios off and an absent SD card.
The same tests check missing sensors, an invalid real-time clock, a stalled analog-to-digital converter, and continued operation.

## Change configuration and diagnose a fault

The factory manual routes network changes through a two-second hold of both navigation buttons and the network setup workflow.
It routes firmware checks through the SenseCraft device and firmware-flasher pages.
The Rust console accepts `interval`, `rtc`, `status`, and `diagnose` through the USB serial bridge.
Production-task tests change the interval, save it transactionally, restart the tasks, and check the restored value.
Diagnostics reports local storage and extension measurements and exercises the status light and buzzer.
Fault tests check visible errors, cleanup, bounded waits, and recovery through the same console.
Wi-Fi and Bluetooth Low Energy remain optional features with explicit unavailable-feature responses in offline builds.

## Develop and recover a screen

The factory manual describes a cloud canvas and template deployment.
The Rust workflow adds a `Screen` implementation and uses the repository Skill and preserved references.
Native and browser previews run the same Rust decisions and renderer with editable simulated inputs and replayable events.
Browser tests exercise keyboard submission, invalid-field focus, update failure, recovery, and offline resources.
Framebuffer transfer tests exercise three screens, four rotations, and all four radio configurations through the production task graph.
The preview supports local iteration without pairing an account or flashing the device.
The Rust workflow requires programming and toolchain installation, while the documented factory workflow offers a graphical editor.
The evidence supports those task-specific differences without assigning a general usability score.
