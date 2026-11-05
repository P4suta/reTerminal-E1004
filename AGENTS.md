# Development

Read [.agents/skills/reterminal-e1004/SKILL.md](.agents/skills/reterminal-e1004/SKILL.md) before changing device behavior, rendering, previews, or hardware adapters.
The hardware reference applies to E1004 only.
Read the skill's preserved source catalog and local manuals before following publisher links.
Keep upstream source bytes and notices intact.
Put interpretations in authored references.
Keep physical validation separate from host tests and compiler results.

Run project tools through `mise x -- just`.
Keep decisions in `device-core`, drawing in `device-ui`, and peripheral effects in adapters.
Keep Wi-Fi and Bluetooth Low Energy (BLE) off in the default firmware.
Use English for code, documentation, and interface text.
Use black and white for the default interface.

Run a failing behavioral test before changing a contract.
Keep source-bound Kani harnesses beside pure production functions and exercise affected mutation tests.
Run `mise x -- just check`, `mise x -- just browser`, and `mise x -- just firmware` before delivery.
The verification reference defines the bounded proof, mutation, fuzz, and interpreter commands.
Record an unavailable check as unavailable.
For every discovered defect, identify the violated contract and why the existing checks missed it.
Repair the shared enforcement boundary and test the affected transition family.
Keep parallel mutation workers in separate Cargo output directories.
Keep verification artifacts owned by their producing task.

Keep README and license files generated from `repository.toml`.
Keep hardware source provenance in the skill's `references/sources.toml`.
Preserve third-party notices.
Don't commit, push, publish, or flash a device without the owner's instruction.
