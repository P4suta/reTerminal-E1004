---
date: 2026-11-06
status: proposed
---

# Separate proof and host verification environments

## Context

Kani's distributed verifier requires the AMD64 Linux environment.
Running every Rust compiler through that environment on Apple Silicon also emulates compilation for mutation tests and interpreters.
Complete mutation selections reached the container limit without a verdict.

## Decision

Keep the pinned AMD64 proof image and add a native host image with the same Rust, cargo-mutants, cargo-fuzz, and Miri versions.
Miri continues to interpret the explicitly selected x86-64 Linux target.
Mutation and sanitizer tests exercise portable host code on the native Linux architecture.
Receipts bind both image specifications and reject interrupted retries.

## Alternatives

Increasing limits would change the resource contract.
More fixed shards preserve coverage and still emulate compilation.
The native compiler removes that cost without removing assertions or inputs.

## Consequences

Two image identities require maintenance.
The source and tool versions remain fixed.
Host mutation results and interpreted target semantics remain distinct evidence.
Reconsider the split if the verifier gains native support.

## Confirmation

Run the complete mutation selections, fixed fuzz campaign, interpreter registry, and source-bound proofs.
Reject stale receipts, missing cases, survivors, and inconclusive attempts.
