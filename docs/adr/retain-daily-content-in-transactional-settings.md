---
date: 2026-11-08
---

# Store daily content in transactional settings

## Status

Proposed for evaluation.

## Context

Daily content must survive deep sleep without a network or SD card.

## Decision

Store six checklist entries and a memo in settings format 3 within the existing sectors.
Read earlier formats with empty content.
Validate printable ASCII, checksum the complete record, and commit last.
Coalesce saves before sleep and preserve edits during an earlier save.
Merge queued configuration changes and acknowledge exact request identifiers.

Short side presses select screens.
Holds browse months or tasks.
The middle button selects today or toggles a task.
Holding the middle button refreshes.
Recognize taps after release to avoid duplicate actions.
Calendar dates use the real-time clock domain.

## Alternatives

SD storage requires a card.
Another journal duplicates recovery.

## Consequences

Power loss before saving restores committed content.
Unchanged wakes avoid writes.
Browser controls follow [labels](https://www.w3.org/WAI/tutorials/forms/labels/), [validation](https://www.w3.org/WAI/tutorials/forms/validation/), and [status messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html).

## Confirmation

Check restart, interruption, calendar boundaries, gestures, and display cancellation through production paths.
