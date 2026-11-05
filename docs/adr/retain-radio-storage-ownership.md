---
date: 2026-11-06
---

# Own initialized radio storage

## Status

Proposed and implemented for evaluation.

## Context

After replacing the packet pool, Miri detected leaked task-waker ownership after a connected radio session ended.
Trouble Host 0.8.0 stores connection, channel, and advertising backing storage in `MaybeUninit` without dropping those fields.
The public API can't release that private storage after cancellation.

## Decision

Preserve the licensed package in `vendor/trouble-host` and record its registry version and source commit.
Replace the backing-storage fields with `Option` and initialize them through `insert`.
Rust then drops their packets and wakers during replacement or resource destruction.
Keep the existing host-state lifetime handling and add no unsafe code.
Apply the same package override to all three workspaces.

## Alternatives

Ignoring leaks leaves resource ownership unchecked.
Graceful disconnect can't always clean up after controller failure.
Retaining task headers globally would hide the missing destruction.

## Consequences

The repository maintains a small vendor patch and its original notices.

## Confirmation

Interpreter cases exercise protocol traffic, forced shutdown, and restart with leak checks enabled.
Reconsider the override after an upstream release preserves ownership and passes those cases.
