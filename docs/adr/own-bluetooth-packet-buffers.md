---
date: 2026-11-06
---

# Own radio packet buffers

## Status

Proposed and implemented for evaluation.

## Context

The real Bluetooth Low Energy (BLE) protocol test exposed a borrow violation in Trouble Host 0.8.0's default packet pool under pinned Miri.
Allocating another packet reborrowed the shared backing array while an earlier raw pointer remained live.
Native execution and compilation missed that shared-buffer violation.

## Decision

Use the library's `PacketPool` interface with buffers owned by each packet.
A synchronized reservation counter limits live packets to sixteen, each with 251 bytes.
Dropping a packet releases its reservation.
Remove the default-pool feature from both runtime and firmware dependencies.

## Alternatives

Keeping the default pool preserves the detected violation.
A vendor fork could repair its interior mutability but adds unsafe source and maintenance.
Owned buffers use the supported interface without unsafe app code.

## Consequences

Packet moves copy larger values, while buffer lifetimes follow Rust ownership.
Capacity stays bounded without heap allocation.

## Confirmation

Capacity, exhaustion, independent contents, movement, and reuse tests complement a counter proof and the real protocol interpreter cases.
Reconsider the default pool after an upstream repair passes those cases.
