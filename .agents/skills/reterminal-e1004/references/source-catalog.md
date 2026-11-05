# Preserved source documents

## Read the saved edition

Seeed Studio remains the publisher of the device documentation.
The checked-in originals identify the edition used for repository work and remain available without the publisher's site.
Read the authored hardware and development references for this Rust firmware, then inspect the relevant saved source.
The [source inventory](sources.toml) records original URLs, source commits, retrieval times, license scope, sizes, and document checksums.
The [checksum manifest](sources.sha256) covers every file under `upstream`.

Keep the originals byte-for-byte unchanged.
The Git attributes preserve their line endings and upstream whitespace while keeping text diffs available.
Put corrections, model differences, and repository decisions in the authored references.
Refresh the originals and their provenance together after reviewing changes to the applicable edition.
A saved edition establishes what the publisher documented at capture.
Physical acceptance establishes operation on a delivered device.

## Complete manuals

The [E1004 manual](upstream/wiki/e1004-manual.md) preserves the complete English Markdown source, including specifications, factory setup, operation, expansion pins, and troubleshooting.
The [Arduino display cookbook](upstream/wiki/arduino-display.md), [peripheral cookbook](upstream/wiki/arduino-peripherals.md), and [real-time clock cookbook](upstream/wiki/arduino-rtc.md) preserve their complete English sources.
The Wiki source commit is `f096f684a19950b30469919a9b0c38c0a8b013cc`.
The original Docusaurus tab markup, examples, links, and model-specific sections remain intact.
Use raw Markdown inspection when a renderer doesn't support the tab markup.

The shared cookbooks include other E Series models.
Select the E1004 section before using a pin, controller, or peripheral example.
Factory SenseCraft procedures describe the supplied factory firmware.
This repository's offline Rust firmware follows the development reference.
The hardware reference records the expansion-connector label discrepancy between the Wiki and schematic.

## Schematic, panel protocol, and figures

The [original schematic](upstream/hardware/e1004-schematic.pdf) contains eight pages and identifies revision 1.0 dated January 5, 2026.
The PDF retains its circuit connections, sheet hierarchy, figures, and printed license.
The [T133A01 definitions](upstream/seeed-gfx/T133A01_Defines.h) and [E1004 setup](upstream/seeed-gfx/Setup523_Seeed_reTerminal_E1004.h) preserve the driver and board source files.
The [Serial Peripheral Interface (SPI) frequency configuration](upstream/seeed-gfx/XIAO_SPI_Frequency.h) preserves the included clock settings.
Their source commit is `0dfdd7135425be82b5bb4b4b58d74dd51ab29a59`.
The setup overrides the generic panel dimensions, so read it alongside the definitions.

The snapshot includes 58 original images from the selected manuals under `upstream/images`.
Inspect the [hardware overview](upstream/images/209.png) and [expansion-header figure](upstream/images/217.png) locally.
An image URL under `https://files.seeedstudio.com/wiki/reterminal_e10xx/img/` maps to the same suffix under `upstream/images` after URL decoding.
For example, `e1004/arduino_1.jpg` maps to [the saved E1004 board-selection screenshot](upstream/images/e1004/arduino_1.jpg).
Directory names and case preserve the source distinctions between E1003, E1004, and shared figures.

The Markdown originals keep publisher URLs in image tags.
Use the local image mapping for offline inspection.
Opening raw Markdown doesn't rewrite those tags automatically.
Embedded videos, the external Arduino logo image, websites, and cloud services aren't mirrored.
Images and the schematic are retrieval-time snapshots from mutable download URLs, while the Wiki and driver text use pinned Git commits.

## Attribution and license scope

Credit Seeed Studio and the Wiki contributors for the product documents and associated figures.
The saved [publisher policy](upstream/licenses/seeed-wiki-policy.md) identifies the [Creative Commons Attribution-ShareAlike 4.0 International License](upstream/licenses/cc-by-sa-4.0.txt) for documents and images.
Embedded examples use the [Massachusetts Institute of Technology (MIT) license](upstream/licenses/seeed-examples-mit.txt) unless otherwise specified.
The archive also includes the [source repository license](upstream/licenses/wiki-repository-gpl-3.0.txt).
Keep the document-specific policy and the repository license together when interpreting the source package.
The driver files keep the full [Seeed_GFX license and component notices](upstream/licenses/seeed-gfx.txt).
The repository's own license doesn't relicense the preserved materials.

The inventory keeps external links to the SHT4x, PCF8563, and SY6974B datasheets.
Redistribution terms for those documents remain unconfirmed.
The authored hardware reference preserves the implemented register, conversion, and validation contracts in original prose.
Espressif, Bluetooth Low Energy, and Web standards references remain external to this device-manual snapshot.

## Verify without the network

From this directory, verify every preserved file with the standard checksum tool.

```sh
shasum -a 256 -c sources.sha256
```

From the project root, search the saved Markdown directly.

```sh
rg -n 'Expansion Header Pinout|RTC|SHT4|E1004' .agents/skills/reterminal-e1004/references/upstream/wiki
```

Inspect schematic text or render the PDF with Poppler when circuit details matter.
Use the original PDF for connections and drawings.

```sh
pdftotext -layout .agents/skills/reterminal-e1004/references/upstream/hardware/e1004-schematic.pdf -
```

Use publisher URLs for newer hardware revisions, releases, advisories, or facts absent from the saved edition.
If the publisher is unavailable, report the preserved edition and any unresolved version question.
