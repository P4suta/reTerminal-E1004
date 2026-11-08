---
date: 2026-11-08
---

# Control the offline device through browser serial connections

## Status

Proposed for evaluation.

## Context

Browser actions must operate the device and report persistence and presentation outcomes.
Default firmware must remain offline.

## Decision

Use one validated command and media-transfer contract through USB serial.
Correlate replies with request identifiers.
Keep flash and display effects behind the existing bus owner.
An active transfer blocks sleep and expires after inactivity.
A bounded control lease permits interactive use without disabling sleep forever.

Use [Web Serial](https://developer.chrome.com/docs/capabilities/serial) after an explicit connection gesture.
Keep network servers and persistent network credentials outside the offline control path.
Reconsider a local network adapter when users need operation without a connected host.

## Alternatives

A cloud relay adds an external account and service dependency.
A simulator-only interface leaves device operation uncovered.
A local network server requires an IP stack, authentication, credential storage, and a separate power policy.

## Consequences

USB browser support depends on the browser.
USB control requires a connected and awake device.

## Confirmation

Exercise fragmented transport, malformed requests, retries, cancellation, shutdown, and restart with modeled ports and target compilation.
