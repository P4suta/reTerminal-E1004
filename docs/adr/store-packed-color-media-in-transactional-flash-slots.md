---
date: 2026-11-08
---

# Store packed color media in transactional flash slots

## Status

Proposed for evaluation.

## Context

The [E1004 image cookbook](https://wiki.seeedstudio.com/reterminal_e10xx_with_arduino_peripherals/) requires palette conversion and bounded image buffers.
Content must survive deep sleep without a host or SD card.

## Decision

Share placement, color conversion, dithering, and packed-image validation in the Rust core.
Decode general image formats and render Unicode text and QR content on the host.
Keep live app decisions and drawing on the device.

Reserve nine 1 MiB flash slots after the existing app partition for eight gallery entries and a staging record.
Use erase-before-program writes, checksums, generations, and a final commit marker.
Never erase a current record while staging its replacement.
Persist gallery selection and slideshow settings with backward-compatible settings decoding.

Keep interface chrome black and white.
Use the requested six colors in displayed content.
Use full refresh because the [E1004 driver](https://github.com/Seeed-Projects/OSHW-reTerminal-Series-E-D/blob/main/examples/base/GxEPD2_reTerminal_E1004/GxEPD2_T133A01_1200x1600.h) disables partial refresh.

## Alternatives

SD-only storage makes a card mandatory.
On-device general image decoding consumes scarce working storage.

## Consequences

Prepared images occupy more flash than compressed originals.
Browser color previews approximate physical pigments.

## Confirmation

Check packing, placement, interruption, readback, deletion, restart, and display completion through production ports.
