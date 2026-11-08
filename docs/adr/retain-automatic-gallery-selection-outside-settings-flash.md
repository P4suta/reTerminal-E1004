---
date: 2026-11-08
---

# Keep automatic gallery selection outside settings flash

## Status

Proposed for evaluation.

## Context

Automatic slides need persistence without frequent sector erasure.
Manual preferences still need durable storage across power loss.

## Decision

Keep manual gallery selection in flash and the current automatic selection in retained working memory.
Use the checksummed settings encoding for a retained snapshot.
Restore only the selection after a button or timer wake, and only when all remaining preferences exactly match committed settings.
Ignore retained state after a power-on wake.

Use the pinned [Espressif persistent memory attribute](https://docs.espressif.com/projects/rust/esp-hal/1.2.0/esp32s3/esp_hal/attr.ram.html) with atomic words.
Invalidate the leading magic before writing and restore it last.
The decoder rejects partial or corrupt snapshots.

## Alternatives

Saving every slide in flash makes automatic progress consume erase cycles.
An append-only flash journal can preserve automatic progress through power loss but adds another transaction and compaction protocol.

## Consequences

Deep sleep resumes the current slide without settings writes.
Power loss resumes the last manual selection.
Reconsider a flash journal when automatic position must survive complete power loss.

## Confirmation

Run repeated production sleep and restart cycles with a populated gallery and require a constant settings write count.
Reject partial snapshots and mismatched preferences.
