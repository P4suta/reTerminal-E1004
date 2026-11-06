---
date: 2026-11-06
---

# Restore the selected screen after quiescence

## Status

Proposed and implemented for evaluation.

## Context

Deep sleep discards navigation.
Displaying before measurement wastes a refresh.
Settings failures must preserve independent SD results.

## Decision

Wait for measurement before display.
Save the selected screen in transactional settings, with a console-selectable Status policy.
Read format 1 with defaults and write format 2 using reserved payload bytes.
Save changed navigation after quiescence, avoiding writes on unchanged timer wakes.
Report failed retention once per desired value through a dedicated event and fault.

## Alternatives

Memory in the real-time clock domain avoids writes but adds target-specific persistence.
Saving each button press increases flash wear.

## Consequences

Screen changes can write flash.
Failed persistence can restore an earlier screen.
Preview measurement uses the same event boundary.

## Confirmation

Runtime tests restart tasks after sleep and compare screens and writes.
Fault tests require injection hits and correct component attribution.
Proofs check display admission and attribution.
Reconsider memory in the real-time clock domain if navigation measurements show unacceptable flash wear.
