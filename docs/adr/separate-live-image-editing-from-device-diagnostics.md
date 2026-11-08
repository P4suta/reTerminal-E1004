---
date: 2026-11-08
---

# Separate live image editing from device diagnostics

## Status

Proposed for evaluation.

## Context

Daily tasks need a short path from content to preview and device display.
Diagnostics and manual preparation obscure that path.

## Decision

Separate Images, Daily, and Device using the [W3C tabs pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
Keep development controls behind an explicit tools URL.
Keep the preview beside controls on wide screens and first on narrow screens.
Preserve each image's source and editing values.
Coalesce edits and admit asynchronous results only for their active request.
Show valid edits automatically without moving focus, following [On Input](https://www.w3.org/WAI/WCAG22/Understanding/on-input.html).
Require an explicit action for downloads and device writes.
Use `PNG` for normal downloads and a separate action for SD export.

## Alternatives

Collapsing the existing sections preserves the preparation dependency and mixes unrelated tasks.
Automatic device uploads would hide a physical operation behind an editing gesture.

## Consequences

Original image sources consume browser memory.
Reconsider background conversion if measured editing latency prevents interaction.

## Confirmation

Test visible pixels after every editing control, image switching, rapid input, invalid input, export, and transfer.
Exercise task navigation, focus, narrow layout, and text enlargement.
