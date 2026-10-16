# Coinbase auth and WebSocket session slice

Moved **19 pending -> ported, 0 -> divergent**, measured with Python 3.11 `tomllib`
against authoring parent `5a488d641d7499ed6a84bd3d766ce86fdc4b85a3`.
Coinbase census: **300/17/109 -> 319/17/90** (ported/divergent/pending).
Enumeration stopped after all 16 Coinbase manifest files and 426 rows at this
snapshot. The 19 changed rows and their real Rust targets are recorded in
`AUTH_SESSION_CENSUS.json`; census is not behavioral proof.

The implementation ports seven auth/provider symbols and twelve controller
symbols. A host supplies eager local producer polling and FIFO microtask wakes.
Completion Promise handles never drive producers. Ordered reactions and queued
continuations preserve await timing, ignored-call progress, activation identity,
generation guards, mutable auth results, and policy reads across awaits. The
existing Future-facing RPC API remains maintained alongside private host Promise
variants. The boundary is declared native callbacks/client/policy types and finite
owned JSON; arbitrary JavaScript objects/getters/prototypes are not covered.

## Measured gates

All measurements below are attached to code SHA
`5241d0ad7baa72945daa457c2b16d7f7eabe6046` in
`AUTH_SESSION_GATE_RESULTS.json`, including commands and log hashes. This commit
was built directly on the reviewed integration parent above; no foreign unreviewed
branch was included as an authoring ancestor.

| Gate | Exit | Observed output |
| --- | ---: | --- |
| Scoped Cargo check, Coinbase, all targets, jobs 2 | 0 | Finished dev profile; zero compile errors |
| Scoped Coinbase tests, all targets, jobs 2, include ignored | 0 | 193 passed, 0 failed, 0 ignored, 38 completed test binaries |
| Extractor check via explicit Node/Yarn path | 0 | 7341 symbols / 512 files; 0 manifest updates, 0 additions, 0 orphans |
| Scoped unchanged native parity checker | 0 | Invention and status gates passed: 426 rows, 522 public native items |

Oracle tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The extractor's output text was inspected, not inferred from the Yarn exit.
The scoped native checker verifies actual Rust targets against actual Coinbase
metadata and exemptions; it makes no workspace-wide mapping claim.

## Discrimination controls

`scripts/auth-session-mutations.py` applies one compiling production mutation at
a time, requires the named failing assertion and failed-test summary, restores
the original bytes in `finally`, and reruns the exact test. Each mutant failed
with exit **101**; each restored test passed with exit **0**.

| Production mutation | Named native assertion caught |
| --- | --- |
| Cache expiry `<` -> `<=` | `native auth result and retained extra fields` |
| Execute await continuation inline | `consumer polling cannot execute activation producer` |
| Disconnect does not increment generation | `native controller state` |
| Ignore policy readback scope mismatch | `native activation completion branch` |
| Dispatch observers before earlier Future waiters | `mixed Promise reactions retain registration order` |

Stopped after these five controls; **no observed survivor**. This is a floor,
not an exhaustive mutation search. Source and tests were verified byte-identical
to the gate SHA after restoration (`git diff --exit-code`, exit 0).

## Native assertion and enumeration scope

The eleven new tests compare native values, error fields and original rejection
carriers, callback counts/order, request/context fields, state, Promise identity,
and shared result/policy mutations to live TypeScript outcomes. No new test relies
on decode/re-encode equality or a wire roundtrip.

Independent fixture floors are 26 auth constructor cases; 29 auth scenarios and
71 operations; two controller constructor cases; 22 activation scenarios; six
state guards; and three disconnect reasons. Enumeration stops at those cases,
plus the explicitly named ignored-call, concurrency, queued-race, reaction-order,
replacement, policy-mutation and retry probes. Test-host draining stops at queue
quiescence with a 1000-turn cap. TS settling probes stop at 12 or 32 microtasks;
they do not establish every possible scheduler interleaving.

A read-only native sub-agent review found no remaining blocker within the stated
boundary and confirmed all nineteen mapped declarations exist. That review ran
no Cargo or mutation controls; the measured exits above were established by the
authoring agent.

Not run: workspace compilation/tests, monorepo suites, host clippy, wasm clippy,
wasm target builds, CI, or live Coinbase network operations.

## Integration merge-down

Merged integration `0f02e1219579430184111306d15e12e7f0864141` cleanly, producing
`3b8317b479f9b783d1cd58eae46b734258e113ef`.
The prescribed source/test diff against the original gate SHA exited **0**;
Coinbase scripts, manifests and TS sources also remained unchanged. The original
193-test and five-control exits carry under the standing preservation rule;
those tests were **not rerun after the merge**.

At merge SHA `3b8317b479f9b783d1cd58eae46b734258e113ef`, separately re-established
the scoped Cargo check (**0**, zero compile errors with updated shared crates),
extractor check (**0**, text inspected: zero updates/additions/orphans), and
scoped native mapping (**0**, 426 rows / 522 native items). Exact commands and
log hashes are in `AUTH_SESSION_MERGE_GATE_RESULTS.json`.

Re-measured the 16-file / 426-row tomllib census against the merged integration
head, not the old merge base: **19 pending -> ported, 0 -> divergent**, still
**319/17/90**.

Immediately before push, fetched and merged the newer integration
`473004e7b056eacb8bb51419d062993624c4e908`, producing
`93e09c480e4b7329bb4e11bce4642eb5e8d25b8f`. Source/test preservation again exited
**0**. The additional integration change only touched the MEXC manifest; the
listed scoped dependency sources, Cargo configuration, Coinbase scripts/metadata,
exemptions and TS sources were preserved (**0**). Tests/controls still carry from
`5241d0ad7b`; scoped compile and native mapping carry from `3b8317b479`.
At `93e09c480e`, the final extractor check independently exited **0**, with its
zero-update output inspected. Final exits and preservation commands are tracked
in `AUTH_SESSION_FINAL_GATE_RESULTS.json`. The final tomllib census in
`AUTH_SESSION_CENSUS.json` compares against integration `473004e7b0`: the true
gain remains **19 pending -> ported, 0 -> divergent**, **319/17/90**.
