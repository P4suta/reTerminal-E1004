---
date: 2026-11-06
status: proposed
---

# Verify immutable source snapshots

## Context

Development and hooks can change a live checkout during verification.
Digests taken before and after execution can miss temporary source changes.
A rejected attempt must never leave a reusable successful receipt.

## Decision

Copy each bounded task's source into a private container workspace before execution.
Verify that copied input digests match their capture.
Keep build output outside source directories.
Publish only task-owned artifacts.
Invalidate successful authority when captured source changes and report the changed input paths.
Bind results to source, toolchain, immutable image, and mutation artifacts.

## Alternatives

Locking the live checkout would require every editor and hook to cooperate.
Digest checks alone admit temporary input changes.
A private copy costs storage and transfer but isolates verification from ordinary editing.

## Consequences

Results describe the captured source and become stale when the checkout differs.
The filesystem, compiler, and container remain trusted boundaries.
Reconsider the copy when a portable atomic filesystem snapshot can preserve the same input contract.

## Confirmation

Inject a source change during receipt publication and restore the original bytes.
The result must remain unusable.
Change the live fixture and require the copy's original bytes to stay intact.
