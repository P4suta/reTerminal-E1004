# Verification and acceptance

## Fast gate

Run `mise x -- just check` for format, strict Clippy, all-feature host tests, the Wasm build, source inventory, and design checks.
The gate compares Cargo's actual firmware-task test inventory with the required case registry before running the tests.
Run `mise x -- just browser` for native button and keyboard operations, field labels, computed styles, replay, narrow layout, and matching native/Wasm frame digests.
The browser check generates screenshots under `target/screenshots` for visual inspection.
Run `mise x -- just firmware` for offline, Wi-Fi, Bluetooth Low Energy (BLE), and both radio features, including merged 32 MB flash images.
A cross-compile doesn't verify electrical behavior or radio connectivity.
`mise x -- just ci host` combines host, dependency, browser, and image-export checks.
`mise x -- just ci device` also builds and lints all four firmware configurations and validates the current bounded-verification receipts.

Run `mise x -- just dependencies` to check all three lockfiles, license requirements, package sources, and secrets.
The task clones an isolated RustSec snapshot and prints its commit.
Vulnerabilities, unsound dependencies, and rejected licenses fail the check.
Maintenance advisories remain visible warnings because the required `esp-hal` 1.2.0 dependency graph includes archived build tools.
RustSec identifies `paste` and `unsafe-libyaml` as unmaintained in that graph.
The project keeps the required vendor version and reports those maintenance risks without marking the dependencies maintained.

Tests cover every pixel in every rotation, both controller halves, pack boundaries, and battery anchors.
They also cover settings corruption, interrupted slot writes, invalid real-time clock values, and sensor cyclic redundancy checks.
Transition tests cover rapid input, coalescing, timeout, and removed storage.
Startup tests require exactly one completed-measurement frame for healthy and absent sensors.
The screen matrix starts fresh production tasks after sleep and checks the restored screen and unchanged flash write count.
The settings fault matrix derives every injection point from a successful transaction and requires each injection to fire.
It checks independent SD status, the settings fault, the unacknowledged interval, and recovery.
The [quality contracts](../../../../docs/verification/quality-contracts.md) map failure classes to their enforcement boundary, independent checks, and unresolved assumptions.
The T133A01 trace test compares the complete initialization sequence with the pinned vendor sequence and checks transfer sizes and phase ordering.
Fault injection exercises a failure at each slot-storage operation and Serial Peripheral Interface (SPI) cleanup after a failed transfer.
The browser comparison uses a fixed event fixture and the SHA-256 digest of the canonical packed framebuffer.

## Bounded verification

The proof image pins Rust, Kani, and the C Bounded Model Checker (CBMC) for AMD64 Linux.
The native host image pins Rust, cargo-mutants, cargo-fuzz, and Miri.
Run `mise x -- just verification-image` to build `verification/Dockerfile` and `verification/Host.Dockerfile` and save their immutable identities under `target`.
Set `E1004_VERIFICATION_IMAGE` or `E1004_HOST_VERIFICATION_IMAGE` to an immutable image identity when selecting a preserved image explicitly.
Miri always interprets the explicitly selected x86-64 Linux target, including when its compiler runs on another Linux architecture.
The project commands use the owner's bounded container runner with 2 CPUs, 2048 MiB of memory, no swap, 256 processes, and a 900-second stop limit.
Each task executes a private source copy whose input digests match its capture.
The copy excludes Git state, build output, and previous results.
On other machines the same project commands use Docker with identical resource limits and a 900-second `timeout` process.
Time limits stop an experiment and leave an inconclusive result.
They never turn a missing result into a pass.

`mise x -- just proofs` imports production Rust into Kani harnesses.
Harnesses declare integer domains and loop-unwinding bounds beside their assertions.
Unwinding assertions remain enabled.
Cover properties check that admitted inputs reach the specified valid and invalid cases.
The core proof inventory covers coordinate bounds, nibble preservation, disjoint controller mapping, battery bounds and monotonicity, settings round-trip and single-bit corruption, update coalescing, and timeout failure behavior.
The board proofs cover sensor conversion ranges, valid calendar encoding, button holds, the console buffer's state invariant, and disjoint settings-sector addressing.
The runtime task tests import the firmware's production effect-dispatch helper and check that a full queue retains its request.
The UI proof checks the production append helper with a four-byte buffer and an arbitrary byte chunk of up to eight bytes.
It then appends one ASCII byte through the text API.
It checks the stored length, exact byte preservation, and visible formatting-error state.
The production writer accepts `str` and delegates its full byte slice to the helper.
Rust's string type and standard-library UTF-8 validation remain trusted boundaries.
The console proof checks one arbitrary transition from every admitted control state, with payload bytes set to zero because the transition never branches on stored payload bytes.
The policy decoder proof checks every ASCII value of up to six bytes using independent byte predicates.
Console tests check both commands with every ASCII trailing byte through the complete parser.
The standard library's string trimming remains a trusted boundary and also receives fuzz inputs.
Kani checks host Rust integer semantics with explicit buffer limits below the 32-bit address range.
The proofs don't model the executor, vendor libraries, physical memory, or interrupt timing.
The preview cancellation proof calls the production event step for an active timeout and a failed late completion.
It covers arbitrary operation identifiers and every orientation.
Each step must return no follow-up event.
Host sequence tests exercise the complete drain loop.
Core proofs also check measurement admission, component-specific settings faults, saved-screen projection, and format 1 compatibility.
Runtime proofs check sleep admission across every quiet boundary, emitted sleep actions, display generations, charger bits, owned packet reservations, and boot recovery with independent slot-read failures.
The boot proof makes failed reads write valid-looking newer records before returning an error and checks that the runtime discards those bytes.
The pinned panel source specifies `BUSY` high as ready.
Software must use and test that specified polarity before device delivery.
Physical acceptance checks the delivered board's signals and measured sensor accuracy.
Flash behavior under a brownout and battery-life prediction also require physical evidence.

`mise x -- just mutations` runs the declared core, preview, portable panel, and production runtime selections without shuffling and with 2 workers inside each 2-CPU limit.
The core runs in two fixed round-robin shards, each with its own bounded container.
The runtime runs in four fixed round-robin shards over its measurement, policy, task, button, console, state, Wi-Fi, BLE, and packet-pool modules.
Each worker uses a separate mutation build directory.
`mise x -- just verification mutations-bsp` checks the console parser through the same captured-source runner and records an owned result.
Inspect every survivor and add a behavioral assertion for a real missing invariant.
Distinguish equivalent behavior, build rejection, timeout, infrastructure failure, and an observable regression.
Don't suppress a production function to raise the mutation score.
`mise x -- just fuzz` uses fixed input counts and seeds for external codecs and event sequences.
The target also checks console and firmware-feature round trips, preview asset paths, and style parsing.
It executes the production console dispatcher with partial and failed writes and parses concrete hardware binding syntax.
The committed corpus remains unchanged while exploration writes a temporary copy.
`mise x -- just miri` interprets supported pure and adapter tests under the pinned nightly.
The interpreted UI test checks that a capacity failure replaces partial text with a visible error.
The six replay tests keep their full-frame comparisons and run as separate bounded cases with at most two cases concurrently.
The gate compares their case registry with Cargo's complete replay-test inventory and requires a named pass for every case.
Runtime measurement and sleep-policy tests run under the same interpreter.
`mise x -- just verification model` enumerates all five-event words over nine app events and runs the restart and settings-fault matrices.
Its report counts 295,245 concrete transitions under that finite bound.
`mise x -- just verification fuzz` wraps the pinned sanitizer campaign in a source-bound result producer.
The owned packet test allocates and releases the complete bounded pool with disjoint inline buffers.
The real BLE host's protocol, restart, and cached-battery scenarios run as separate bounded cases without removing assertions.
Aliasing and leak checks stay enabled.
Neither fuzzing nor Miri establishes a theorem for unexecuted inputs or hardware.

## Completion before device delivery

Separate published requirements, executed software checks, and measurements from the delivered unit.
Host tests execute the production firmware task graph from boot through measurement, console input, display transfer, persistence, quiescence, and restart.
The matrix combines all four radio configurations, four rotations, and three screens.
The model uses delayed operations and real Embassy wake notifications rather than polling every future unconditionally.
The real Trouble Host processes modeled controller packets and central requests.
Wi-Fi tests check controller operations, bounded failure, and resource release under the SDK peripheral contract.
The source binding gate and four target builds check the concrete hardware adapters.
The [peripheral-port decision](../../../../docs/adr/execute-firmware-tasks-through-peripheral-ports.md) records model assumptions and trusted dependencies.

The [target-emulation experiment](../../../../docs/verification/target-emulation.md) records the actual Espressif QEMU release, downloaded checksum, rejected 32 MB image, diagnostic boot, and unsupported clock and peripheral boundaries.
Full execution of the distributed target binary remains unsupported by that emulator release.
The checked alternative executes production control paths through modeled ports and checks the target adapter's source and compilation.

The [workflow comparison](../../../../docs/verification/workflow-comparison.md) compares documented factory procedures with equivalent offline Rust tasks.
Keep physical button feel, viewing quality, real refresh latency, and measured user preference separate from that source-based comparison.
The device record contains actual observations rather than a placeholder list of experiments without an available acquisition path.

## Result identity

Keep command arguments, tool versions, source digests, selected harnesses or mutants, bounds, seeds, and actual exit status with each result.
The source identity includes the producing operation and verification code, toolchain, lockfile, and container specification.
Receipts also bind the selected immutable container image.
One scope registry drives generation and validation.
Mutation artifacts must match their recorded SHA-256 digest.
File lengths frame the source bytes before hashing, so a file payload can't impersonate another file boundary.
Source changes during capture invalidate successful authority even after the original bytes return.
Starting a retry invalidates that task's earlier receipts without changing another task's artifacts.
Regenerate a result after changing a covered production function or its assumptions.
Keep unrun, failed, inconclusive, and physically unverified checks distinct.
Run `mise x -- just check-assurance` to reject missing, stale, unsuccessful, or incomplete local verification receipts.
The hardware reference limits device checks to commands and observations the agent can access through the connected interfaces.
Unavailable visual, electrical, and manual experiments aren't outstanding software work.
The [software verification record](../../../../docs/verification/software-checks.md) reports the implemented scope and observed results.
