# Quality contracts

## Presented images and asynchronous input

An update can expose a new image only after the active operation completes successfully.
Input changes the requested screen while the previous image remains visible.
Repeated hold operations, cancellation, timeout, late success, and completion-mode changes preserve that rule.
The pure presentation type owns completion authority and the preview exposes read-only state.

The original core proofs covered app actions but didn't cover the preview's choice of frame.
The earlier preview test covered ordinary completion and omitted transitions between failure and completion modes.
The fix removes frame selection from completion-mode flags.
The app is the sole owner of orientation.
Typed configuration events and recorded configuration commands must produce the same app state and pixels.
The regression test compares both entry points in every orientation.
The production completion helper has a source-bound Kani harness, while preview tests reproduce both the repeated-hold and timeout-mode failures.
The harness treats an eight-bit snapshot as an opaque payload.
The algorithm treats snapshots as opaque values and uses Rust's copy semantics.

The independent presentation model schedules eight control operations through every sequence of length six and every prefix.
The campaign covers 262,144 complete sequences.
The model uses its own screen indices, pending request, and completion rules.
It doesn't call the production transition or presentation functions.
The finite sequence bound leaves longer histories to fuzzing and separate source-bound transition properties.
Periodic measurement scheduling remains covered by the app transition tests rather than the short-sequence model.

## Measurements and recordings

The original browser checked submitted errors but didn't test native constraint enforcement or errors surviving unrelated actions.
The form bypassed native validation, and every action cleared field validation.
Native form validation and the shared Rust draft validator now reject out-of-range submissions without changing the frame or recording.
The browser exercises every numeric field's rejected boundaries, keyboard submission, error persistence, correction, and mutually exclusive completion selection.
The native date control normalizes zero seconds away.
The preview preserves minute-precision dates by adding only zero seconds before the existing calendar parser.
A source-bound proof checks byte preservation and suffix construction for every byte value at input lengths up to 20.
The existing parser remains responsible for calendar validity and the supported century.

A measurement submission validates every field before changing app state, display requests, or the recording.
The browser accepts temperature in degrees Celsius and relative humidity from 0% to 100%.
Exact decimal parsing avoids floating-point rounding.
Tests cover every supported temperature value, valid and invalid length boundaries, all form choices, and field-specific errors.
The length boundary tests use valid leading-zero numbers to check length rejection without numeric overflow.
Replay errors identify the original line and never commit a valid prefix of an invalid log.

Accepted recordings fit the replay byte budget, including line separators.
Kani checks the production admission predicate with wider reference arithmetic.
An independent test covers boundary combinations.
Command, form, and replay fuzzing checks failed-operation atomicity and canonical recording replay.
Fixed corpus entries preserve the measurement transaction and timeout-mode reproducer.

## Peripheral failure injection

The console rejected unknown or invalid commands without explaining recovery and provided no help command.
Its earlier tests checked command execution and output failures without requiring discoverable instructions.
Help now reports formats and actual interval limits, while static parse-error messages direct the operator to help without echoing input.
The production console tests cover partial writes, failed output, malformed text, oversize input, recovery, and exactly one rejection response.
The earlier corpus asserted that unavailable radios stayed inactive without requiring available radio commands to emit their requested effects.
The dispatcher corpus now checks both capability cases, and the console-loop matrix checks successful and failed responses for every rejection path.

The panel test injects a transport failure at all 3,236 fallible operations across initialization, both controller transfers, and sleep.
It checks that transfer cleanup runs, later phases stop, the original error survives cleanup failure, and another display requires successful reinitialization.
The independent vendor trace fixes command order, transfer lengths, controller routing, and executor yields.
An empty or shortened protocol fails the injection-count assertions.
The fake transport models successful bus release when its cleanup method runs.
The runtime task tests add genuinely pending delay and ready futures, timeout cancellation, and final sleep rechecks.
The delivered unit's electrical chip-select behavior remains a device acceptance condition.

The storage campaign interrupts every storage operation and every committed-record prefix.
Recovery retains a valid old or new setting after interruption.
The raw-flash model follows the pinned storage adapter's sector read, erase, and program sequence.
It injects 49,154 byte-boundary interruptions across ordinary and wrapping generations and checks the untouched active sector independently.
The independent FAT32 reader checks written files and card removal.
Sensor tests flip every bit in three complete sample frames and compare all 65,536 raw sensor words with a signed rational reference.
The checksum anchor comes from section 4.4 of the [Sensirion SHT4x datasheet](https://sensirion.com/resource/datasheet/sht4x), edition 7.3 dated June 2026.
The same edition's section 4.6 supplies the conversion equations.
These checks establish codec behavior and fault handling under the transport assumptions.
They don't establish sensor calibration, flash behavior during a physical brownout, or electrical timing.

The earlier console round-trip check started with parser output.
It couldn't detect a valid command rejected before decoding or bytes discarded during trimming.
The parser now preserves the tab separator and password spaces.
Generated valid credentials must decode exactly, including an empty password.
The source-bound obligation checks every pair of byte lengths.
The first mutation selection for the board support package also included fixtures compiled only for Kani.
The fixtures now compile for unit tests, whose independent state table checks preserved constructor fields, state admission, and one-byte transitions.
The Kani harness checks constructor inputs before assuming the state invariant.

## Mutation execution and result authority

The former board support package mutation shortcut mounted the live workspace instead of using the shared source capture.
A firmware build removed an object while a mutation worker copied that build directory.
The board support package selection now uses the same immutable workspace copy, source identity, artifact binding, and owned-result registry as the other mutation selections.
The scope test requires its receipt, while the shared snapshot test rejects build directories at every workspace level.

Mutation testing covers the core, preview, portable panel driver, and production runtime tasks.
Two fixed round-robin core shards keep each experiment within the container budget while retaining the complete selection.
Four runtime shards cover all nine production modules.
The selection includes both radio tasks and owned packet storage.
All shards use the same pinned tool, arguments, source selection, and denominator under the [cargo-mutants sharding contract](https://mutants.rs/shards.html).
A resource stop remains inconclusive.
The gate rejects survivors, timeouts, empty selections, failed baselines, missing selected files, duplicate mutant identities, and inconsistent totals.
Compiler rejections remain distinct from mutations caught by tests.

Earlier mutation results used a shared Cargo target directory and aren't retained as quality evidence.
The command builder removes inherited Cargo output-directory settings for mutation runs.
Each worker then uses the tool's own isolated build directory.
An executable command-construction test prevents reintroducing the shared directory.
Each campaign runs the actual tests on every mutated implementation.

Receipts bind the producing operation, covered source, verification implementation, fixed toolchain, lockfile, and container specification.
Mutation receipts also bind the exact outcome artifact by SHA-256.
One scope registry serves both receipt generation and validation.
Each bounded task transfers only its own artifacts, so an older container snapshot can't overwrite another task's newer result.
Tests check scope completeness, artifact ownership, shard selection, malformed reports, false totals, failed baselines, survivors, and timeouts.
The fast gate imports the production receipt validator into an isolated integration-test process.
Temporary fixtures exercise changed artifacts, swapped subjects, changed product and producer files, unsupported formats, and missing or unsuccessful exits.
They also replace the immutable container identity while retaining an otherwise valid receipt.
Starting a retry removes only that task's receipts before launching its container, so an interrupted retry can't inherit an earlier successful result.
The negative control removes that invalidation and must fail while another task's result remains byte-identical.
Source-change validation also precedes successful receipt authority.
A changed capture writes an unusable result with no successful exit, even if the original source bytes return later.
The earlier publication order wrote a successful exit before checking source stability, allowing a rejected attempt to become reusable after restoration.
The regression changes a captured file, rejects publication, restores the original bytes, and still requires refusal.

Each file's content length frames its bytes before hashing.
Without a length field, two different file inventories had the same serialized input.
One file contained another file's encoded name, delimiter, and payload.
A two-file fixture and a merged one-file fixture demonstrated the collision.
Both inputs reached SHA-256 as identical byte sequences.
The length-framed implementation distinguishes the fixtures.
SHA-256 collision resistance and the operating system's prohibition on zero bytes in file names remain explicit trust assumptions.
Two injected faults remove artifact binding or replace a subject's name with its length.
Each fault must trigger the regression assertion.
The count-admission predicate has a source-bound Kani proof over arbitrary 64-bit counters with wider reference arithmetic.
The pinned JSON parser, SHA-256 implementation, compiler, verifier, and container runtime remain trusted dependencies.

## Interpreter completeness

Miri runs the six replay tests as separate cases while retaining every assertion and full-frame comparison.
The earlier combined run reached its resource limit and supplies no passing evidence.
Each case keeps the same pinned interpreter and resource limits.
The runner executes at most two replay cases concurrently.
An independently captured Cargo test inventory must match the complete case registry, including unique test names.
Every case must report a passing result for the registered test with no ignored tests.
Empty, duplicate, missing, unexpected, skipped, and incomplete work fails the gate.
Tests and corpus replay exercise the inventory boundary.
Cargo's test-list format and Miri's reports remain trusted tool interfaces rather than formally verified parsers.
The Bluetooth Low Energy (BLE) protocol, controller restart, and cached-battery scenarios also run in separate bounded interpreter cases.
Their registry preserves every scenario from the earlier combined test, which reached the resource limit without a verdict.

## User workflows and limits

The browser exercises native buttons, Enter submission, invalid-field focus, labels, descriptions, mode state, actual canvas preservation, recovery, reset, and atomic replay.
It compares native and Wasm frames across all three screens, four rotations, and five measurement scenarios.
The 60 comparisons establish agreement between adapters.
Independent checks use geometry, the reviewed SHA-256 value, protocol traces, and rendered output to detect shared rendering errors.

The earlier screenshot check compared packed frames but didn't inspect the saved display image in Portable Network Graphics (PNG) format.
Visual review found an element screenshot with an incomplete, scaled crop.
The artifact now comes from the canvas at its intrinsic 1200 by 1600 resolution.
The browser decodes the saved PNG and compares its color and alpha bytes through their SHA-256 digest with the native frame.
Valid PNG controls with a crop or one changed pixel must fail that comparison.
The reference frame is an immutable fixture result, and each matrix case has its own preview instance.
Page screenshots still capture the actual desktop and narrow layouts.

The interface uses native form and disclosure controls, visible keyboard focus, ordinary measurement units, concise correction messages, and a separate display-status announcement.
The development reference records the applicable HTML and W3C sources.
Desktop and narrow screenshots require visual inspection after layout changes.
Automated accessibility checks don't establish complete accessibility or comparative usability.
Published factory procedures and comparable preview tasks support a workflow comparison before device delivery.
The [workflow comparison](workflow-comparison.md) records prerequisites, local status, configuration, diagnosis, preview, and recovery from the preserved factory manual and executed Rust workflows.
Separate observations establish button feel, viewing quality, refresh latency, and user preference.

## Production task composition

The original startup transition emitted measurement and display actions together.
The task tests accepted any nonempty list of frames, and the pure proof only excluded overlapping display operations.
Neither condition excluded displaying unavailable values before measurement.
Display admission now requires measurement completion, and repeated Refresh requests share the current measurement.
The startup regression requires exactly one frame after a healthy or missing-sensor measurement.
The stress regression supplies 1,000 input requests during measurement and requires one final display before sleep.
A source-bound proof excludes measurement, display, and sleep actions while a measurement remains outstanding.
The finite event model independently checks measurement ownership, navigation indices, fault identity, display ownership, and sleep admission over every five-event word from nine event kinds.

The earlier screen matrix supplied a wake reason at boot but stopped after sleep.
The separate restart scenario checked interval recovery from the default screen.
Neither scenario checked navigation across a newly constructed task graph.
The matrix now restarts production tasks for every radio, orientation, and screen combination.
Separate cases cover every published wake reason and both resume policies.
The settings format keeps backward compatibility while the runtime commits changed navigation after quiescence.
Assertions check the persisted screen and forbid another flash write on an unchanged wake.
Policy-switch tests also forbid replacing the active screen with the prior policy's projected screen.
The first policy-switch case covered one direction.
The expanded case switches both policies from every active screen and checks the saved value before sleep.
Changing only the policy for screen restoration preserves the refresh deadline and starts no display operation.

The earlier runtime fixture declared a settings-failure switch without executing it in its task tests.
Low-level interrupted-write tests established record recovery without checking the app's diagnostic projection.
Failure to save settings entered the SD status event and never updated the app's last error.
Dedicated settings-failure events now preserve the SD result and identify settings flash explicitly.
SD failures independently record their own fault, while successful card status updates preserve error history.
Diagnostics must redraw after a fault identity change even when the card status stays the same.
The runtime matrix measures the successful transaction's operation count before injecting a fault at each operation across all three card states.
Every case requires exactly one injection hit, correct diagnostics, unchanged active settings, and a successful later save.
Automatic retention failures use the same handler and must stop retrying the same failed value before sleep.
The test registry makes removal or omission of these scenarios fail the required gate.
The first affected core mutation run exposed a gap between package boundaries.
Runtime tests checked projected settings, while core-only mutation runs didn't execute those tests.
The core contract now checks every screen policy, screen, orientation, and interval boundary through the settings projection.
A second gap recorded the fault without asserting which screen should redraw.
Core tests now check error attribution, redraw admission, and pending work across every screen, measurement state, and display state.
The settings-fault proof checked attribution without redraw admission.
Its source-bound assertions now check both properties and preserve work in progress.
The preview drains simulated measurement and display events through an explicit loop over the production event step.
Recursive dispatch and the monolithic drain-loop proof each exceeded the fixed memory limit without a verdict.
The source-bound proof now starts with an active display, checks timeout cancellation, changes completion mode, and checks an arbitrary late completion through the production step.
Both steps must return no follow-up event.
Host sequence tests exercise the complete dispatcher.

The earlier preview and portable driver tests used a different scheduler and immediately ready transport futures.
Compilation checked the target types but never executed the firmware's composition of signals, queues, cancellation, and sleep.
That boundary allowed a sleep predicate to require a flag already consumed by `Action::Sleep`.
Repeated sleep requests also occupied a queue that the sleep predicate required to be empty.
The runtime now coalesces tickets in a signal and derives admission from the emitted app state, active generation, configured interval, and individual quiet conditions.
Kani checks the production predicate over arbitrary admitted conditions, and a separate harness checks an actual emitted sleep action.
The task tests execute the same production futures as the target entry point with delayed ports, virtual time, and real wake notifications.

The earlier task checks sent a display-completion event for a sleep-time panel failure while the app was idle.
The display-completion transition ignored that event while idle.
Panel failure reporting now revokes sleep and accepts faults only for the active generation.
The transition tests cover idle, updating, and failed states, every fault, stale identifiers, and wrapping identifiers.
The task test injects the actual sleep transport error and requires a visible fault with no deep-sleep entry.
The core proof covers current and stale failure generations.
The busy-panel test had a masked-result gap.
The timer task had already reported the expected timeout before the adapter returned.
Separate adapter cases omit the timer task and check deadline, cancellation, invalid frames, transport errors, and cleanup directly.
The fault policy uses an exhaustive error match with a source-bound proof, so an added driver error requires a classification decision.

Console output and controller shutdown need a shared quiescence contract.
The earlier console acknowledged a flush but still drained later logs, and a scan could finish after that acknowledgment.
The console now writes only during normal operation or an explicit flush barrier.
The bus task requests a final barrier after controller release and rechecks all conditions after panel sleep.
BLE setting consumption always wakes the shutdown waiter, including an off request before controller initialization.
A finite-poll unit test requires the setting future to remain pending without input and acknowledge both Boolean values.
The task tests require scan-completion logs to reach the flushed console and forbid writes after an acknowledgment until another barrier.

The earlier models kept an advertising flag after controller drop and injected button events without the general-purpose input/output conversion boundary.
Those assumptions let premature controller shutdown and inverted button polarity escape detection.
The controller lease now stops advertising when dropped, and independent packet fixtures assert address, flags, service bytes, battery reads, notifications, disconnect, and reconnect behavior.
Physical input tests drive all three active-low inputs through bounce, hold, release, and repeated presses and observe the production button task's events.
Removing input inversion triggers the first released-input assertion.
The BLE server uses the pinned library's automatic request processing on event drop rather than duplicating acceptance behavior whose removal was equivalent.

The original boot fault model supplied two read errors and returned after the first.
It never exercised one unreadable slot beside a valid readable slot.
Boot now reads both independently through a typed result that exposes bytes only after success.
The test matrix fails each slot independently and makes failed reads write a newer valid-looking record before returning an error.
The source-bound proof checks the same production boot function with arbitrary read failures and storage error variants.
Recovery preserves the valid readable setting and reports the storage fault without trusting failed buffers.

Writing a 24-byte settings record triggers a larger physical program operation in the pinned flash adapter.
The adapter reads a 4096-byte sector, erases that sector, and programs the reconstructed contents.
For example, committing the inactive sector must preserve the active sector.
The inactive sector begins at address `0xa000`.
The active sector begins at address `0x9000`.
A logical slot mock that replaces only the requested bytes misses sector overlap and reconstruction failures.
The raw-flash fixture interrupts erase and program progress at every byte boundary for all three transaction writes.
It requires the active sector to remain byte-for-byte unchanged and recovery to return a valid previous or new record.
The address helper used by the real flash adapter has its own disjoint-range proof, and the target image generator parses the same partition table.
Those independent checks connect record recovery, physical addressing, and packaging under the published flash and storage contracts.
Analog corruption during a brownout remains a physical measurement boundary.

Console fault injection originally represented a blocked transmitter only as a permanent failure.
Removing its retry arm still reached the expected failure and left a later healthy message writable, so the old assertions accepted that regression.
The expanded test blocks the initial banner for 500 milliseconds, requires zero lost messages, then permits partial writes before the one-second deadline.
The banner must complete exactly once rather than disappear or restart as a separate message.
The flush test independently checks a temporary wait, a complete deadline expiry, the missing acknowledgment on failure, and recovery through another explicit barrier.
An empty receiver produces no read error, while an injected bus error produces a diagnostic and still permits a later command.
The code distinguishes a temporary wait from a serial error with exhaustive matches.
The compiler then rejects a missing error variant, while behavioral tests establish retries, progress validation, deadlines, and recovery.
The distinction prevents a passing permanent-failure test from standing in for a temporary transmission wait contract.

The earlier integration fixture used the default interval, masking a dropped loaded setting, and compared transferred pixels with current desired state after sleep.
The matrix now checks independent interval and orientation anchors before events, then compares controller bytes with the last successful presentation snapshot.
It covers four radio configurations, four orientations, three screens, and power, timer, and button wake reasons.
The hardware binding gate also checks actual signal roles and initial levels, because a pin-number inventory alone accepted a pin attached to the wrong peripheral method.
Metamorphic fixtures require wrong roles, levels, tokens, duplicate pins, and missing initialization to fail.

The fast gate captures Cargo's complete all-feature task-test list and requires every registered case.
A separate syntax-based test requires the source cases and registry to match uniquely.
Without a required registry entry, the actual gate exited unsuccessfully before test execution.
An ignored test appeared in Cargo's list, and the earlier gate succeeded after executing nineteen cases and skipping one.
The source inventory now rejects ignored cases and conditional attribute injection.
The injected ignored case failed the required-case assertion after that correction, and the restored case executes normally.
The firmware gate generates merged images through the real partition parser after building each feature configuration.
That operation exposed invalid custom settings partition types that the Rust compiler never reads.
The persistence checks combine the corrected partition table, disjoint-address proof, raw-flash interruption tests, and image generation.

## Executor progress and radio ownership

The original virtual executor drained ready tasks until quiescence before advancing time.
A task that wakes itself could consume that entire budget and freeze timer progress.
Ordinary task tests reached quiescence, so the executor's own scheduling error escaped those tests.
The executor now advances time after a fixed poll quantum while polling only tasks with a real wake notification.
A regression combines a permanently yielding task, a three-millisecond timer, and a task that receives no wake notification.
The timer must finish.
The task without a wake notification must remain unpolled after its initial suspension.

Once the executor progressed, Miri reached the real BLE host's packet and connection lifetimes.
The library's default packet pool formed packet slices through a mutable pool borrow.
A second allocation invalidated the first packet's pointer under the checked aliasing model.
The app now supplies packets that own their fixed-capacity byte arrays and share only a checked reservation counter.
The packet test allocates the complete capacity, compares disjoint buffers, releases every packet, and rejects allocation beyond capacity.
Kani checks reservation and release bounds over every eight-bit counter value.
Mutation testing exposed that the native counter assertions covered only valid counts, so the native test now independently checks all 256 values with wider arithmetic.

The next Miri run completed BLE assertions but reported a leaked task wake state.
The upstream resource container initialized backing stores for connections, channels, advertising, and bonds.
Those stores used `MaybeUninit`.
The container never destroyed the initialized values.
The repair gives each backing store normal ownership through `Option`.
Initialization uses `insert`.
The host's manually managed stack-state lifetime remains unchanged.
The regression drops the executor, host, controller, and system.
Every saved weak reference to a task's wake state must report zero strong owners.
The unpatched dependency fails that assertion, while the repaired dependency releases the owners.
The three BLE scenarios also run under Miri with aliasing and leak checks enabled.
The [dependency provenance record](../../vendor/trouble-host/UPSTREAM.toml) identifies the exact publisher archive and modified source.

An assertion that diagnostics ended with the light and buzzer off admitted a mutation that never turned either output on.
The diagnostic test now requires both outputs on before the pulse deadline, still on immediately before expiry, and off at expiry.
The ready, failed, and absent storage cases each require diagnosis to finish.
The injected admission defect fails the initial on-state assertion.

## Preserved sources and development hooks

The first attempt to commit ran a hook that changed publisher documentation in the working tree and staged index.
Restoring from the index retained those rewritten bytes because the index had changed too.
The independent source manifest rejected the mismatch and blocked the documentation commit.
Recovery read the attested original Git tree and verified every preserved file's checksum.

The common hook now verifies owner-registered archive or manifest identities from the same Git snapshot as the source under review.
Verified publisher bytes remain untouched, while added findings and changed authored comments still pass the normal checks.
Changing a parser body requires a parser owner even when its signature and structural finding stay identical.
Tests reject substituted archives, escaping paths, links, repeated members, duplicated inherited findings, overlapping registrations, and failed registry edits.
The registration establishes provenance rather than correctness of publisher code.
Registry persistence serializes updates with a process lock and replaces a synced temporary file after validation.
An explicit lock owner releases the lock even when another descriptor remains alive.
Closing only the original descriptor left a parallel test temporarily blocked, and retaining a duplicate descriptor made that failure deterministic.
The [Rust file-lock contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.unlock) distinguishes closing all inherited or duplicated descriptors from explicitly unlocking.
The regression keeps a duplicate alive and requires an independent opener to take the released lock.

The verification runner binds every result to an immutable image identity and source inputs.
The AMD64 proof environment runs the distributed Kani verifier.
Native Linux compilers run mutation and sanitizer tests, while Miri interprets the explicitly selected x86-64 Linux target.
The split removes emulated compiler work while retaining pinned versions, inputs, assertions, and resource limits.
Temporary dependency build output must stay outside preserved source directories because the source identity includes the complete dependency tree.
The identity check rejected attempts that overlapped a separate dependency feature check and its cleanup.
Bounded tasks now copy the checkout to a private container workspace and verify that the copied inputs match their capture.
Build directories, Git state, and previous verification results never enter that copy.
The capture rejects symbolic links and pre-existing destinations.
Tests change the live source after capture and require the copy's original bytes to remain intact.
Each task publishes only its registered artifacts after executing its captured source.

The first source-bound fuzz adapter read only standard output from a successful native receipt.
LibFuzzer wrote its completion record to standard error, so the adapter rejected a completed campaign.
The shared completion predicate now requires a successful exit and a complete record on either process stream.
Regression cases cover each stream, missing stream fields, failed exits, empty or incomplete counts, and fragments split across streams.

## Target and physical boundaries

The [target-emulation experiment](target-emulation.md) records an actual attempted boot and the available emulator's flash-capacity and clock-model failures.
The [peripheral-port decision](../adr/execute-firmware-tasks-through-peripheral-ports.md) states the checked alternative and its SDK contracts.
The target compiler and source binding gate check concrete adapters while host tests execute production control paths under those contracts.
Host execution doesn't establish silicon behavior or a complete model of every possible scheduler history.

The former acceptance template listed physical properties without an acquisition path.
The documentation gate checked reference presence and source metadata without checking whether an experiment was executable.
The [hardware reference](../../.agents/skills/reterminal-e1004/references/hardware.md#physical-acceptance) now limits agent work to reachable commands, observable results, and explicit acceptance criteria.
The device record no longer contains placeholder experiments.
Don't classify unavailable visual, electrical, or manual measurements as unfinished agent work.
Keep verification of pin selection, register values, control order, wake configuration, cancellation, and recovery independent of physical observations.
