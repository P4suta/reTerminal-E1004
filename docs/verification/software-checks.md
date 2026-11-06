# Software verification

Recorded on November 6, 2026.
This record describes the verified baseline at commit `b7ac579`.
The current lifecycle contracts and required inventories appear in the [quality contracts](quality-contracts.md) and [verification reference](../../.agents/skills/reterminal-e1004/references/verification.md).
The software checks passed for the offline app, shared rendering, previews, portable adapters, and compiled ESP32-S3 firmware configurations.
Physical device acceptance remains unverified.
The [quality contracts](quality-contracts.md) explain each discovered defect, the earlier enforcement gap, and the regression boundary.

## Integrated checks and preview

The integrated device command exited successfully.

```sh
mise x -- just ci device
```

It passed formatting, strict host Clippy, host tests, Wasm compilation, source inventory, and monochrome design checks.
The device profile also passed dependency and secret checks, browser checks, image generation, four Xtensa build and Clippy configurations, and current bounded-verification receipts.
The firmware configurations are offline, Wi-Fi, Bluetooth Low Energy (BLE), and both radios.
The required end-to-end registry contains 21 tests that execute the production task futures through peripheral ports.
The firmware composes the same tasks with Espressif hardware adapters.
The compiler checks those concrete adapters in every radio configuration.
The binding gate compares pin roles, active levels, initialization, and adapters with the saved hardware contract and rejects representative changes.
The [target emulation experiment](target-emulation.md) reached app entry, then stopped in clock initialization before the tasks ran.
Espressif QEMU rejects the distributed 32 MB flash image and lacks the required peripheral models.
Execution of the complete distributed binary remains unsupported by that emulator release.

The browser command passed native form submission, Enter submission, invalid-field focus, labels, descriptions, completion-mode state, timeout recovery, reset, and atomic replay.

```sh
mise x -- just browser
```

Failed form submissions preserve the app, frame, and recording.
Invalid replay reports its original source line and commits no prefix.
The main navigation buttons stay within the desktop and narrow viewports.
The preview requests local resources without an account or external service.

Native and Wasm rendering agreed across three screens, four orientations, and five measurement scenarios, giving 60 frame comparisons.
The scenarios include ordinary values, boundaries, missing hardware, faults, and stale measurements.
The fixed status fixture's digest of packed pixel bytes remains `c61b27399647450c8dc3710d9151a9912554305b7ac97b41141aa1a108211af9`.
The browser decodes the saved display image in Portable Network Graphics (PNG) format at 1200 by 1600 pixels.
It compares the decoded color and alpha bytes with the native frame's SHA-256 digest.
Valid PNG controls with a crop or one changed pixel fail that comparison.
Desktop and narrow page screenshots still capture the actual layout.
Visual review covered the desktop and mobile page captures and the complete display image under `target/screenshots`.
The native window also presented and closed after the requested two frames.
The native help command and PNG generation completed successfully.
The computer-control service couldn't attach to the native window, so operating-system keyboard input remains unverified.
The shared input behavior and browser keyboard paths have executable checks.

The earlier element screenshot contained an incomplete crop after scaling.
The frame comparisons missed the artifact error because they didn't decode the saved file.
The artifact check now reads the written file and compares its dimensions and content.
An immutable fixture frame and a separate preview instance per matrix case prevent the reference from inheriting the last case's state.

## State transitions and fault injection

The independent presentation model explored 262,144 complete six-event sequences and every prefix across eight control operations.
It uses its own screen indices, update scheduler, pending state, and completion rules.
It never calls the production transition or presentation functions.
The sequence bound excludes longer histories and periodic scheduling, which have separate transition and fuzz checks.

The panel campaign injected transport failures at all 3,236 fallible operations across initialization, both controller transfers, and sleep.
Each failure stops later phases, releases the modeled transfer, and preserves the primary error when cleanup also fails.
Another display requires successful reinitialization.
The test asserts nonempty baseline counts of 21 initialization operations, 3,212 display operations, and three sleep operations.
An independent vendor trace checks command order, controller routing, transfer lengths, and 3,200 executor yields during a refresh.

The storage tests interrupt every operation and every incomplete committed-record prefix.
Recovery keeps a valid old or new setting after interruption.
The raw-flash model injects 49,154 interruptions at erase and program byte boundaries across ordinary and wrapping generations.
It checks the untouched active sector independently and models flash programming as clearing bits.
The FAT32 test uses an independent file system reader and removes the modeled card at each write boundary.
Sensor tests reject all 144 single-bit changes across three complete sample frames.
They compare all 65,536 raw words with signed rational conversion equations and check the publisher's checksum vector.
The checksum vector follows section 4.4 of the [Sensirion SHT4x datasheet](https://sensirion.com/resource/datasheet/sht4x), edition 7.3 dated June 2026.
Section 4.6 supplies the conversion equations.

Geometry tests check every pixel in every orientation, reject aliasing, and check orientation anchors and controller coverage.
Measurement tests round-trip all 17,501 supported temperature values and check grammar, exact length boundaries, field errors, and every form choice.
The complete packed-byte test checks all 256 inputs with the documented opaque palette as its reference.
Boundary tests cover rapid input, display coalescing, late completion, timer expiry, invalid real-time clock values, removed storage, and sleep quiescence.

Production-task tests add genuinely pending operations and virtual time.
They exercise boot, measurement, full pixel-buffer transfer, console commands, settings persistence, shutdown, and restart with all four radio combinations, four rotations, and three screens.
Faults cover a permanently busy panel, cancellation, panel sleep failure, events arriving during shutdown, rejected wake configuration, failed console flush, missing sensors, stalled conversion, absent pixel buffers, and card removal.
The executor advances time after a bounded number of ready polls and polls tasks again only after their notification signals readiness.
A regression checks timer progress while another task keeps yielding.

Wi-Fi tests run the production scan, connect, disconnect, timeout, and shutdown tasks through the same generic controller interface as the target adapter.
BLE tests execute the real host and Attribute Protocol over an independent controller packet fixture.
They check advertising, battery reads, notifications, disconnect, reconnect, cached measurements, controller failures, and restart.
Shutdown must release every owned task notification.
The diagnostic task checks the light and buzzer before and after their pulse deadline for ready, absent, and failed storage states.
Console tests check partial and pending output, invalid progress, bounded line parsing, output failure accounting, and the final log-flush barrier.

## Source-bound proofs

Kani 0.68.0 with the C Bounded Model Checker (CBMC) 6.11.0 checks production Rust.
It verified thirteen core harnesses, five board harnesses, one UI harness, two preview harnesses, one tool harness, and ten runtime harnesses.
All 32 required harnesses passed.
The separate false-contract control failed its assertion as intended.
The inventory file and the tool's harness registry define the expected nonempty work.
The inventory path is `verification/inventory.toml`.

Under the harness assumptions, the core proofs establish coordinate and address bounds, injective rotation, nibble preservation, and controller separation.
They also cover battery bounds and monotonicity, settings round-trip and single-bit corruption, redraw coalescing, timeout and retry behavior, and sleep quiescence.
The added core harnesses check completion authority and cancellation, exact fixed-point decimal construction, and rejection of non-decimal bytes without a panic.
The presentation harness uses arbitrary eight-bit snapshots because the production algorithm treats its generic payload as opaque and only copies it.
The decimal construction harness covers signed values with whole parts up to 999 and two fractional digits.
The invalid-input harness covers arbitrary eight-byte ASCII strings containing a non-decimal byte.
Neither harness claims the complete Unicode string domain.

The board proofs check sensor arithmetic, valid calendar encoding, stable button behavior, console buffering, and disjoint flash-sector addresses.
The console-state proof checks one transition from each admitted state with zeroed stored payload bytes because control flow doesn't inspect the payload.
The UI proof checks capacity transitions, exact bytes, and visible formatting failure with arbitrary byte chunks of up to eight bytes in a four-byte buffer.
The proofs trust Rust's string type and standard-library UTF-8 validation.
The preview proofs compare recording admission with wider arithmetic and forbid publication after cancellation, mode changes, or a late success.
They also check the app's ownership of orientation for every admitted configuration.
The tool proof checks mutation-count admission over arbitrary 64-bit counters with wider reference arithmetic, including sum overflow.
The runtime proofs check current sleep authority, quiet-boundary admission, emitted sleep actions, and active display generations.
They also check erased and corrupt settings, charger bits, bounded packet reservations, and boot recovery with every combination of failed slot reads.
Failed reads may write valid-looking newer records before returning an error.
The boot harnesses check that the production recovery function discards those bytes.

Each harness declares its input domain and unwinding bound beside the production assertions.
Unwinding assertions remain enabled.
The verdicts cover host Rust semantics under those assumptions.
They don't establish the complete executor, vendor libraries, interrupts, physical memory, sensor calibration, or electrical behavior.
The executed task tests and Miri cover selected executor and vendor-library interactions beyond those proofs.
The campaign trusts the JSON parser, SHA-256 implementation, Cargo report formats, compiler, verifier, and container runtime.

## Mutation, fuzz, and interpreter results

The earlier report of 219 mutations is withdrawn because its workers shared a Cargo output directory.
A broken implementation exposed a false passing result caused by worker artifact reuse.
The command builder now removes inherited Cargo output-directory settings, and each worker uses the mutation tool's own build directory.
The executable command-construction test guards that boundary.
No accepted survivor, exclusion, or reduced threshold replaces the correction.

The two core shards completed 250 mutations: tests caught 246 and the compiler rejected four.
Neither shard had a survivor or timeout.
The preview selection completed 121 mutations: tests caught 113 and the compiler rejected eight.
The preview had no survivor or timeout.
The panel selection completed eight mutations, all caught by tests with no survivor or timeout.
The four runtime shards completed 184 mutations: tests caught 167 and the compiler rejected seventeen.
The runtime selection includes both radios and owned packet storage.
Across all selections, tests caught 534 of 563 mutations and the compiler rejected the remaining twenty-nine.
No selection had a survivor or timeout.
All selections use cargo-mutants 27.1.0, fixed source paths, two isolated workers, and no shuffling.
Each test run has a 60-second timeout.
Two fixed round-robin core shards keep the complete selection within the container budget.
Four fixed round-robin shards cover the complete runtime selection.
The declared result gate requires no survivor or timeout, a successful baseline, consistent totals, unique identities, and work for every selected file.

A previous equivalent decimal-parser mutation exposed an unnecessary signed digit counter.
The parser now expresses the required condition as a Boolean that records whether a whole digit occurred.
Public tests cover exact byte limits independently of numeric overflow.
Other survivor analysis added assertions for field-specific errors, every status choice, direct controls, byte packing, recording separators, and display feedback.
The fix removes redundant capacity arithmetic and an identical color branch rather than excluding them from mutation testing.

Cargo-fuzz 0.13.2 completed 10,000 runs with seed 104, input lengths up to 8192 bytes, and AddressSanitizer.
The target exercises codecs, interrupted settings recovery, console and firmware-feature round trips, exact decimals, atomic command and replay failure, and canonical recording replay.
It also exercises malformed mutation reports, interpreter inventories, preview paths, style parsing, event transitions, and malformed bitmap input.
The runner replays a temporary copy of the committed corpus and leaves the source corpus unchanged.

The pinned Miri interpreter passed all twenty-two selected tests using the nightly compiler dated August 21, 2026.
They include six replay cases with complete frame comparisons and six board-boundary tests.
The remaining cases cover panel-failure cleanup, text-capacity failure, five runtime boundaries, and three complete BLE scenarios.
Aliasing and leak checks remain enabled.
The first BLE runs exposed invalid packet aliasing and leaked initialized connection storage.
Owned packet buffers and the preserved dependency's minimal storage-ownership correction passed the same scenarios.
The [quality contracts](quality-contracts.md) record the original counterexamples, publisher archive identity, and unchanged regression assertions.
The original combined replay run reached its resource limit and supplies no passing evidence.
All six replay tests keep their assertions and full-frame comparisons in separate bounded cases.
The runner compares Cargo's complete replay-test inventory with the case registry and requires a named pass with no ignored tests for every case.
Neither fuzzing nor Miri establishes a theorem for unexecuted inputs or hardware.

## Evidence integrity and limits

Kani uses the immutable image `sha256:43308014aeb2c2d98f2c046fdbd81b4957e5fc7eeb5426fc18746924f7d9a1ae`.
Native mutation, sanitizer, and interpreter campaigns use `sha256:8d4954f74a2944b2390e2e0c288561a825dd62cf215564241ab14ab834e293bd`.
Miri interprets the explicitly selected x86-64 Linux target.
Each container has limits of two CPUs, 2048 mebibytes of memory, no swap, 256 processes, and 900 seconds.
A resource stop is inconclusive and never counts as successful evidence.
Each task captures a private source copy and compares file manifests before capture, after capture, and before dispatch.
Generated outputs and live source edits can't change another task's private inputs.
The source identity includes the producing operation, covered files, verification implementation, toolchain, lockfile, inventory, container specification, and actual immutable image.
Receipt format 3 encodes each input's byte length before its content to keep record boundaries unambiguous.
Mutation receipts also bind the exact outcome artifact by SHA-256.
One scope registry drives both generation and validation.
Each task transfers only its own artifacts to prevent older snapshots from replacing another task's results.

The ignored results directory stores generated commands, source digests, actual exits, reports, and mutation artifacts.
The directory path is `verification/results`.
The receipt gate rejects stale, missing, failed, empty, or incomplete results.

```sh
mise x -- just check-assurance
```

The current source identities matched all required receipts.
Replacing one core shard's receipt with the other shard's receipt failed with `stale verification result: core-mutations-1`.
Changing a valid mutation artifact failed with `mutation artifact does not match its receipt`.
Both controls restored the original bytes, confirmed matching SHA-256 digests, and passed the gate again.
The fast host suite now imports the production validator into a separate integration-test process and exercises temporary fixture data.
It checks artifact and subject binding, changed product and producer sources, unsupported receipt formats, and missing or unsuccessful exits.
The expected assertion failed after removal of artifact binding or substitution of a subject identity with its name length.
The test and the receipt gate passed again after restoration of the exact source bytes.
The gate also rejects a result whose inputs changed during execution, even after the original bytes return.
Retry preparation invalidates the selected task's old result without deleting another task's evidence.
Tests exercise an actual source-framing collision, private-copy isolation, capture failure, symbolic links, image identity, failed publication, and retry ownership.

The pinned Espressif dependency graph reports maintenance advisories for `paste` 1.0.15 and `unsafe-libyaml` 0.2.11.
Xtensa linking reports a load segment with read, write, and execute permissions.
The maintenance and linker warnings remain unresolved.
The GitHub workflows haven't run on hosted infrastructure.
The explicit document prose check passed for the quality contracts and development references.
The editorial and linguistic engines also ran over the quality contracts.
Review retained six technical-term candidates with individual primary-source evidence.
Copyediting verification reran the engines successfully.

## Workflow comparison and software limits

The [workflow comparison](workflow-comparison.md) compares the preserved factory instructions with executed local status, diagnosis, and screen-development tasks.
The Rust workflow removes pairing and network prerequisites for built-in local screens and adds shared previews and replay.
It also requires a build and initial flash.
Seeed preinstalls the factory firmware, which offers a graphical editor.
No measured general usability ranking follows from that comparison.

The [verification reference](../../.agents/skills/reterminal-e1004/references/verification.md) records input bounds and hardware assumptions.
The tests exercise production task control paths and selected task schedules through peripheral ports.
They don't establish every possible schedule or execute the Espressif SDK's physical effects on a complete emulated board.
The compiler, source bindings, vendor traces, independent models, fault injection, proofs, and interpreters supply complementary evidence for the stated software contracts.

## Measurements from the delivered unit

Hardware observations must establish actual panel colors, refresh duration, current consumption, sensor and battery calibration, and physical radio connectivity.
The software requirements include published pin assignments, ready polarity, register decoding, wake configuration, and recovery logic before device delivery.
Measurements stay unverified until a device acceptance run supplies the results, firmware identity, and test conditions in the [device acceptance record](../acceptance/device-measurements.toml).
Accessibility beyond the exercised workflows, physical interaction quality, and measured user preference also remain unverified.
No battery-life or physical-operation claim follows from the software results.
