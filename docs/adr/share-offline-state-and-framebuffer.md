---
date: 2026-11-05
---

# Share offline state and framebuffer

## Status

Accepted in the implementation plan.

## Context

The device must render offline, and previews must reproduce its decisions and pixels.
The framebuffer needs external memory.
The panel and SD card share Serial Peripheral Interface (SPI) signals.

## Decision

Keep the app transition function in a dependency-free `no_std` crate and drawing in a separate `no_std` crate.
Adapters own effects.
Transform orientation before packing the external-memory framebuffer.
One bus task serializes panel and SD operations.
Two settings slots preserve a committed record across interrupted writes.

## Alternatives

Host-rendered images require networking.
Independent SPI owners permit overlapping selection of shared wires.

## Consequences

Shared rendering needs independent geometry checks alongside adapter comparisons.
Physical behavior still requires acceptance measurements.

## Confirmation

Framebuffer comparisons, geometry proofs, vendor traces, and fault injection check the software contracts.
Reconsider bus ownership if a board revision supplies independent buses.
