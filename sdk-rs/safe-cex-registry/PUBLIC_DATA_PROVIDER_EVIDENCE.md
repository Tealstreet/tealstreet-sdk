# CORE/P4a public-data provider slice

Actual parent `1f6e115b0cb3aec1c42860a0f97cf71bd84376d8`; fresh branch `feat/rust-core-p4a-public-data`.
Gate tree `0805dbdee318f96f743d424af2f5d4aa48511a25`. No unreviewed feature branch was used as an ancestor.

## Inventory and movement

Original eleven assigned manifests at parent: 127 rows, 0 pending / 20 ported /
106 divergent / 1 not-applicable (tomllib, iteration 1 fixed eleven-file floor).
Adjacent provider inventory adds 38 pending rows. This slice moved
**9 pending->ported and 29 pending->CHOSEN divergent**, with real individual
function targets. No previously mapped status was changed.

| Manifest | SHA | Total | Pending | Ported | Divergent |
| --- | --- | ---: | ---: | ---: | ---: |
| exchanges/in-process-public-data.ts.toml | 1f6e115b0c | 13 | 13 | 0 | 0 |
| exchanges/in-process-public-data.ts.toml | 0805dbdee3 | 13 | 0 | 5 | 8 |
| exchanges/external-public-data.ts.toml | 1f6e115b0c | 36 | 25 | 8 | 3 |
| exchanges/external-public-data.ts.toml | 0805dbdee3 | 36 | 0 | 12 | 24 |

At the gate tree the fixed thirteen-manifest floor is 176 rows:
0 pending / 37 ported / 138 divergent / 1 not-applicable. These are manifest
counts, not a proof of every source input or all project/core ownership.
A separate parent scan stopped at iteration 2 after files declaring
safe-cex-registry ownership: registry.ts, registry.native.ts and
external-public-data.ts. In-process had no declared Rust owner before this port;
its source's P4a blocker note placed it in this adjacent slice. No other pending
venue or facade rows are credited.

## Exits measured at the gate tree

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --all-targets --jobs 2`: **0**.
- `python3.11 sdk-rs/safe-cex-registry/scripts/public-data-controls.py`: **0**; three controls, serial Cargo only, each exact-byte restored. Each restored run executes `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test public_data_providers --jobs 2` with the required TypeScript path: **0**, 5 passed / 0 ignored.
- Filtered-liveness mutation: compiled **101** at `PUBLIC native liveness before filter`; restored **0**, 5 tests.
- Ping-threshold mutation: compiled **101** at `PUBLIC native ping exact threshold excluded`; restored **0**, 5 tests.
- Reconnect-reset mutation: compiled **101** at `PUBLIC native reconnect reset allows each turn`; restored **0**, 5 tests.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript node sdk-rs/safe-cex-registry/tests/public-data.cjs`: **0**. Actual provider source modules, actual jsonParse helper and source config initializers execute with injected platform drivers and fixed module-load random value.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**; output read: 7341 symbols / 512 files, would update 0 manifests, 0 added/orphaned.
- Native source/tests restoration diff and `git diff --check`: **0**.
- Tomllib status transitions, exact rename-token and canonical bucket checks: **0**, 9/29 movement over exactly two provider files.
- Query/transport/exchange source diff against the parent: **0**; no query Value conversion shim or query projection added.

Extractor writer ran after manifest changes: **0**, updated 2 manifests,
0 added/orphaned. Earlier prototype test compilation exited **101** for an
unused Market import and E0506 test fixture borrow; fixed before the gate tree.
Five tests passed at implementation commit `4b4bda01bc`; all three controls also
passed their 101/0 proof there. Source outbound-field assertions were then moved
after all source native assertions in `0805dbdee318f96f743d424af2f5d4aa48511a25`, and the complete three-control
proof was repeated at the actual final gate tree above. No earlier measured
exits are relabeled as different executions.

## What the native and source witnesses observe

In-process: initial state, seed tickers before markets, actual array identity,
first notification after seeding, filtered in-place value changes visible through
parent aliases, same-array length mutation staying filtered (source behavior),
liveness stamped before filtering, empty notifications not stamping, replacement
snapshots, exact stale threshold, delegated subscriptions and reusable cleanup,
repeated connect retaining earlier subscription, disposal, cleanup/update refusal,
and timestamp zero. Gateway: typed decoded ticker/book/candle/market native
fields, append/replacement and loaded flags, adaptation, subscriber identity,
live Set add/remove iteration, first-error abort, unsubscribed malformed data
ignored, status/error/pong handling, auth and reconnection subscription order,
colon-bearing symbol keys, mutable candle unsubscribe options, ping suppression,
strict stale/ping thresholds, invalid/foreign traffic liveness, and late open/error
behavior. Rust and source native assertions precede their outbound field checks.

Reconnect enumeration stops after a 99-close streak, invalid inbound traffic,
100 allowed post-reset reconnect turns, then the rejecting 101st close. Tests do
not imply exhaustive timer, payload, or callback interleavings. Mutation enumeration
stops at control 3; no survivor was observed among these three attempted controls.

## Declared CHOSEN boundaries and remaining integration

Provider bodies use explicit parent/source/socket/timer capabilities with typed
ExchangeError Result channels, replacing platform objects and arbitrary JS throws.
The host binds the source socket globals and module-load reconnect delay; timers
must enqueue rather than run callbacks inline. Weak callback captures stop effects
on provider drop. Alias handles preserve array identity within a batch; native
records exclude cross-array per-item aliases, callback mutation of typed payloads,
arbitrary getters/prototypes and arbitrary event values. Snapshot loaded flags
are owned; live source loaded-object mutation during seeding is outside this host
domain. Gateway event data is text; malformed typed payloads with active subscribers
are refused. Status/error fields are strings or missing, not arbitrary JS coercions.

Concrete alias-preserving parent/source/socket/timer bindings and BaseExchange/
facade assembly remain separate. An owned-Vec BaseExchange adapter was considered
and removed before commit: it would leave parent prices stale when the source
hotfilter skips an in-place delta. The gate explicitly observes the parent native
price changing through an alias without a full update. No cloning Store adapter,
new Store implementation, or end-to-end venue construction is claimed.

Read-only sub-agent review found eager unsubscribed decoding, inaccurate in-process
subscription notes, and missing nonzero-streak reset coverage. All were fixed and
confirmed. The reviewer ran no Cargo or mutation gates; this lane ran the exits
above. The nine ported rows are four accessors per provider plus in-process
noteDataObserved; all host-dependent method/class/constructor rows remain CHOSEN.

Not run: workspace build, full monorepo suites, registry:check, Rust parity checker,
host/wasm clippy, wasm target, CI reruns or unrelated existing registry tests.

## Integration merge and rerun

Merged integration `0650e658b488165c0a49081aecce3114a35e2861` down as `38c6b4b05e1496fa6ee0414a1ccc49e773c34121`.
Provider source/tests/control script and the two provider manifests are
byte-identical to `0805dbdee3` (diff exits **0**). Dependency source and Cargo.lock
changed, so scoped gates were rerun at the actual merge SHA above:

- scoped safe-cex-registry all-target Cargo check / jobs 2: **0**.
- scoped public_data_providers Cargo tests / jobs 2 / TypeScript path set: **0**, 5 passed / 0 ignored.
- required extractor check: **0**; output read: 7341 symbols / 512 files, 0 drift/added/orphaned.
- tomllib movement against merged integration: **0**, 9 pending->ported / 29 pending->divergent.

The mutation proofs remain measured at their original `0805dbdee3` tree;
these are preserved byte-identical provider and gate implementations, not
new mutation executions claimed at the merge SHA.
