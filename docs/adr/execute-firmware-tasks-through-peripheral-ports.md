---
date: 2026-11-06
---

# Execute firmware tasks through peripheral ports

## Status

Proposed and implemented for evaluation.

## Context

Portable tests and compilation leave firmware task composition unexecuted.
The [emulation experiment](../verification/target-emulation.md) found incompatible flash capacity, clocks, and peripherals.

## Decision

Put production Embassy futures in `device-runtime` behind peripheral ports.
The target entry point and host tests call the same tasks.
Host ports model delayed protocols, faults, and cleanup with virtual time.
The real Trouble Host processes controller packets.
One bus owner serializes display, storage, and sleep after quiescence.

## Alternatives

A separate host scheduler leaves composition untested.
Full target emulation needs unavailable models.
Another SoC emulator duplicates vendor dependencies without electrical evidence.

## Consequences

Source binding checks and target compilation cover concrete adapters.
Compiler, SDK, Embassy, and hardware contracts remain trusted.
Electrical behavior requires measurements.

## Confirmation

The [task matrix](../../crates/device-runtime/tests/e2e.rs) covers boot, input, display, persistence, shutdown, and restart across radios, rotations, and screens.
The [verification record](../verification/software-checks.md) records actual gate results.
Reconsider emulation when its required models become available.
