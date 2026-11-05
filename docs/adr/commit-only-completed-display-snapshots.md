---
date: 2026-11-05
---

# Commit only completed display snapshots

## Status

Proposed and implemented for evaluation.

## Context

An e-paper display keeps its previous image during updates and failures while requested state changes.
Reading requested state from a framebuffer getter bypasses completion authority.

## Decision

Use `device_core::presentation::Presentation` to keep presented and active snapshots.
Only the active operation's successful completion replaces the presented snapshot.
Cancellation revokes the active completion.
Keep state private and completion mode separate from frame selection.
Validate measurement transactions and recording capacity before committing input.

## Alternatives

Drawing requested state directly saves a snapshot but publishes unfinished content when completion mode changes.

## Consequences

Previews preserve the previous frame after failure or cancellation at the cost of snapshot storage.
Physical panel behavior remains an acceptance boundary.

## Confirmation

Source-bound proofs, an independent sequence model, browser checks, and firmware task tests cover completion, late success, and recovery.
Reconsider snapshot storage if a display protocol requires incremental presentation.
