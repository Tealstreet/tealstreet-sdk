# Coinbase RPC slice gate sheet

Pre-merge full-suite and mutation gate SHA: `b315eb1bcb4ec4bb09852b59bf37948521aed7ad`.
Authoring parent: integration `5b5d9004c528addfc33e1ff47ad6961ac5dc94d8`.
The branch resumes our un-PR'd RPC WIP (`ed48a4cde7`, cherry-picked from `9ca75e33b8`);
that implementation is included in this slice's review and gates, not treated as previously verified.
The recovery evidence copied alongside it records the earlier merged recovery and adds no row gain.

## Rows and scope

Python 3.11 `tomllib` census, comparing every Coinbase manifest row identity against the authoring parent:
**21 pending -> ported; 0 pending -> divergent**.
Before: **279 ported / 17 divergent / 130 pending**.
After: **300 ported / 17 divergent / 109 pending** (426 rows).
`RPC_CENSUS.json` records the independent parent statuses and all 21 changed row targets.

New families: shared RPC params/request and response DTOs; transport/sender callback types;
HTTP factory; RPC rejection class and constructor; JSON-RPC client, constructor, transport,
buildRequest, request and parseResponse; heartbeat class, constructor and three operations.
The already-ported error response/version are reused and contribute zero new rows.

Boundary: finite owned JSON and local, non-Send Rust callbacks. Params and request handles retain
caller/transport mutations. Transport and sender rejection carriers retain identity. Callbacks and
request IDs advance before the returned future is polled. Response frame object alias identity,
non-JSON JavaScript values, JS stacks and TypeScript generic ergonomic equivalence are not claimed.
The source record generic is erased to the shared params handle; its raw input is validated by
build_request, and the public request DTO cannot instantiate an unrestricted scalar generic.
Outbound DTOs expose no derived Deserialize. The current exchange does not activate Starbase.

## Gates at the stated SHA

| Gate | Exit | Measured result |
| --- | --- | --- |
| Coinbase `cargo check --all-targets --jobs 2` | 0 | Scoped crate check, zero errors |
| Coinbase `cargo test --all-targets --jobs 2 -- --include-ignored` | 0 | 182 passed / 0 failed / 0 ignored; stopped after all 36 test binaries completed |
| `parity:extract:check` | 0 | Inspected output: 7341 symbols, 512 files; 0 updates, additions or orphans |
| Scoped Coinbase Rust parity target/invention gate | 0 | 426 rows; 489 indexed public Rust items |

Coinbase check/test Cargo commands were sequential, scoped and used `--jobs 2`.
The earlier cached parity-tool `cargo run` had no explicit jobs argument; its log reports no compilation.
Oracle environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn extractor output was inspected, not judged solely from process status.

The original repository-wide mapping attempt at `affaf900b8` exited **1**:
`safe-cex-webworker/src/twap_manager.rs: item macro thread_local cannot be indexed without expansion`.
This remains a disclosed blocker to the workspace mapping claim, not a green gate.
The scoped mapping runner uses the same parity checker, original Coinbase source files, all Coinbase
manifests and original Coinbase exemptions in a temporary workspace with only Coinbase as an inventory
member. It changes no source/row/exemption contents and rejects a changed independent row floor (426).
It establishes Coinbase target existence and public item coverage, not other crates' coverage.
Three existing anonymous normalization/error helpers lacked provenance entries; explicit exemptions
now identify those native shapes and preserve their already-declared CHOSEN behavioral divergences.

## Discrimination controls

Seven sequential controls against the stated clean SHA; each mutation compiled, ran its named test,
exited **101** at the recorded assertion, then passed after byte-for-byte restoration (**0**).
`RPC_GATE_RESULTS.json` records the commands, SHA, diagnostics and mutation diff hashes.
The mutation driver is `scripts/rpc-mutations.py`. There were **zero survivors** among these seven
controls; no claim is made about mutations not enumerated here.

| Control | Named assertion |
| --- | --- |
| Deep-copy params when building the request | `request.borrow().params.shares_object(&input)` |
| Advance ID by two | `native request ID` |
| Add one to RPC error code | `native RPC error code` |
| Replace returned response result with null | `decoded native RPC result` |
| Retain endpoint trailing slashes | `native HTTP URL` |
| Add one to heartbeat interval | `native request params` |
| Restore unrestricted request generic | `request uses the shared params handle, never an unconstrained scalar generic` |

Enumeration stopped after these seven controls and their seven restored runs. The runtime oracle
floors are 13 client constructors, 20 method/params builds, three exhaustion/precedence steps,
26 response cases, 12 HTTP cases, 12 RPC error constructor cases, 12 heartbeat constructors and
16 heartbeat operations, plus shared-object/eager-call/rejection witnesses. These are independent
fixture expectations, not exhaustive input-domain proofs. Native assertions precede wire checks;
HTTP wire checks are deferred until every native case and sender-error assertion has completed.

A read-only sub-agent review found no runtime blocker within this JSON boundary and identified the
request-generic issue, which was corrected and given its own discriminating control. It did not run Cargo.

Not run: full workspace compilation/tests, host clippy, wasm clippy, wasm compilation, live venue
network requests or CI reruns. Workspace Rust mapping has the recorded failed attempt above.

## Integration merge and affected gates

Merged integration `d3f2bb3940811e315aa9e4ab9d970f6b3c08783a` down into this branch.
The initial source/test comparison to `b315eb1bcb` exited **0**, but shared dependency inputs changed.
The additional scoped check at merge SHA `ef887c7b3f` exited **101**: the newly added
`Ticker.timestamp` field was missing in the Coinbase formatter initializer.
At `340c5dd9e97502ff11ea7e3c2ef4b1fa55e67b3c`, the formatter initializes it to **None**, matching
its TypeScript result's omission, and the ticker test asserts that native absence.

Affected gates measured at `340c5dd9e97502ff11ea7e3c2ef4b1fa55e67b3c`:

| Gate | Exit | Measured result |
| --- | --- | --- |
| Scoped Coinbase check, all targets, jobs 2 | 0 | Zero errors with merged dependencies |
| RPC, decoder boundary, and spot CFM ticker tests, jobs 2 | 0 | 9 passed / 0 failed / 0 ignored; stopped after all three requested test binaries |
| Extractor check | 0 | Inspected output: zero updates, additions and orphans |
| Scoped Coinbase Rust targets/invention gate | 0 | All 426 rows; 489 indexed public native items |

`RPC_MERGE_GATE_RESULTS.json` records both the failed initial check and corrected exits.
The **182-test full suite was measured at b315eb1bcb, before this merge**; it was not re-run after
integration's dependency changes. The affected checks above are the post-merge claim.
The RPC implementation/tests and seven control call sites themselves remain unchanged since b315eb1bcb.
A fresh tomllib comparison against merged integration d3f2bb3940 still finds **21 pending -> ported,
0 -> divergent**, with **300 / 17 / 109** across all 16 Coinbase manifest files.
