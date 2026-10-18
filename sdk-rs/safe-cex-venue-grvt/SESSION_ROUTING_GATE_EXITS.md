# GRVT session routing gate exits (historical snapshot)

Base: `3a957718fe445a833bb10a464e16e7cc6c585276` (integration). Fresh branch:
`feat/rust-grvt-session-routing`. No unreviewed feature branch is an ancestor
before this slice. Concrete Axios/socket bindings and websocket class assembly
remain pending; these nine rows port source bodies against bound host operations.
No shared type/trait changed.

## Census

Python 3.14 `tomllib`, matched symbols by TS name against the base's manifests:
164 GRVT rows. Base: 87 ported / 48 divergent / 29 pending. This slice:
96 ported / 48 divergent / 20 pending. **9 pending -> ported; 0 -> divergent**.
Targets: private checkLastReceived, connectAndSubscribe, refreshSessionAndReconnect,
throttledFetchPositions, executePositionFetch; createAPI, createEdgeAPI,
createTradingAPI; lastSegment. Census is not a claim of complete runtime parity.

## Measured exits and revisions

Cargo was serialized throughout, with `--jobs 2`, crate `safe-cex-venue-grvt`,
manifest `sdk-rs/Cargo.toml`, binary `/opt/homebrew/opt/rustup/bin/cargo`.
Every oracle run used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Node 24.20.0 was the default; Node 26.7.0 used PATH prefix
`/opt/homebrew/opt/node@26/bin`.

At `37c0ec81cb3207f7e5d8132137311ad10efe7fb8`, first scoped test command exited
101 during compilation: two unused test helpers. No assertion gate passed there.
At `e4f5a8f9f2c873d963c546ecc603188d7b854abf`, test target session_routing exited 101: the native instrument's
`await value settled` assumption was premature. Corrected the host to wait for
its owned promise. This was a test-instrument failure, not mutation evidence.
At `172454b71b405a41ba834ea298688c981ced982f`, session_routing Node 24 and
scoped all-target check each exited 0. The later pre-try logger witness was
not included at that revision.

At `6efa02879116addef9521f8653aea366bab8d5fc`:

- `cargo test --test session_routing --test private_commands --test private_feeds`
  (with common arguments above), Node 24: exit 0, 9 + 9 + 9 tests.
- Nine one-at-a-time production mutations below: each named native assertion
  failed, cargo exit 101; byte-exact restoration and the same exact test each
  exited 0. Source/tests were restored, scoped working diff exit 0.

At `772216e02f6c5144fb387deca749d25f7c0ad1d1`:

- `cargo check --all-targets` with common arguments: exit 0, zero errors.
- `cargo test --test session_routing`, Node 24: exit 0, 9 tests.
- `cargo test --test session_routing --test private_commands --test private_feeds`,
  Node 26: exit 0, 27 tests.
- Pinned `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  exit 0; output: 7341 symbols / 512 files, would update 0 manifests,
  0 added, 0 orphaned. Judged output text as well as the measured exit.

The writer was run after manifest edits; its output updated exactly three
manifests, 0 added/orphaned. The difference from control SHA 6efa to gate SHA
7722 in src/tests is exactly three trait-contract doc lines; tests and
executable bodies are unchanged. Writer-maintained file arrays were committed
at 7722. Controls are attached to 6efa, successful gates to 7722.

## Discriminating controls

Each command was the common cargo test command with `--test session_routing`
plus the named test filter and `-- --exact`. All broken/restored exits: 101/0.
No survivors. No wire or re-encoding assertion preceded these native assertions.

| Test | Deliberate change | Named failure |
| --- | --- | --- |
| watchdog_source | reconnect threshold 90000 -> 90001 | watchdog/reconnect eager effects native array shape |
| connect_source | missing-host marker true -> false | connect/missing eager effects native array shape |
| refresh_source | attempt increment 1 -> 2 | refresh/success state native number |
| throttle_source | scheduled delay subtract -> add elapsed | throttle/later eager effects native number |
| fetch_source | retain pending-position flag | fetch/success state native bool |
| market_api_source | Market -> Edge factory gateway | market/production eager effects native object shape |
| edge_api_source | Edge -> Market factory gateway | edge/production eager effects native object shape |
| trading_api_source | adapter cookie rm=true -> rm=false | trading/headers-set result native string |
| last_segment_source | first split '?' -> '#' | segment/query result native string |

## Enumeration and limits

Actual TS declarations execute without rewriting method bodies. Independent
expected group counts: 11 / 7 / 13 / 9 / 8 / 10 / 8 / 18 / 8 = **92 cases**.
Floors stop respectively at watchdog/reconnect-throws, connect/error-event,
refresh/log-throws, throttle/clock-throws, fetch/clock-throws, market/raw-error,
edge/raw-error, trading/raw-error, segment/multiple-query. These are bounded
fixtures, not an exhaustive enumeration.

Native assertions cover values, original options/exchange/instance identity,
method receivers, returned config/response identity, wrapped original-error and
response identity, positions identity through Store and sync, state, and ordered
initial/final effects. The response-getter witness stops after proving the TDG
no-response path reads response once. The logger witness proves the per-factory
reauthentication flag remains set when the warning before try throws. Synchronous
retry invocation errors enter catch; asynchronous returned-promise errors do not.

Read-only sub-agent review approved the corrected production bodies; reviewer
ran no cargo/oracle gates. Review found an extra TDG error.response read, corrected
before the successful gates and backed by the alternating getter witness.
The native host owns detached tasks and timer callbacks. Concrete browser/network
adapters, arbitrary host getters/coercions and all async interleavings are not
proved. The date primitive fixtures stop at epoch/invalid-date; no NaN payload-bit
claim is made. Source-prescribed swallowed ping/fetch exceptions remain as written.

Not run: workspace check, full crate/monorepo suites, host clippy, wasm check,
wasm clippy, CI, live exchange calls, browser tests, concrete adapters.

## Integration merge preservation

Merged integration `2a8f733506ff1bb167076694e18a3032e96088a1` into the slice,
result `e8f3c0671891bb498790c7a9e8cceea402bd5f3a`. At that result:

- `git diff --exit-code 772216e02f6c5144fb387deca749d25f7c0ad1d1 HEAD --
  sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests`: exit 0.
- Same comparison for venue Cargo.toml, workspace Cargo.toml, and complete
  safe-cex-exchange/types/store/num/transport/crypto directories: exit 0.
- Same comparison for GRVT TS sources, types.ts, createPositionId.ts,
  LogSeverity.ts and jsonParse.ts: exit 0.
- Cargo.lock changed only the OKX package dependency list (indexmap/crypto).
  No GRVT dependency resolution changed. Scoped cargo exits above still stand;
  no workspace compilation claim.
- Pinned extractor check: exit 0; 7341 symbols / 512 files, would update 0
  manifests, 0 added/orphaned. Output inspected.
- tomllib census and move comparison repeated: 96/48/20, nine pending -> ported.
- `git diff --check`: exit 0. Tracked working tree clean; only the user's three
  untracked .aimux instruction files remain.

Pre-push fetch advanced integration again to
`9916944cd54d8861db6b81f96878fa6082ff4739`. Merged result:
`310bc5955be8e3fa58cf9861c92a6393ea02395e`. Repeated all scoped source/tests,
crate/dependency, oracle-input comparisons above against 7722: exit 0 each.
Also compared GRVT manifests: exit 0. Cargo.lock still differs only for OKX.
Pinned extractor check at 310b: exit 0, same 7341/512 census, would update 0
manifests, 0 added/orphaned; output inspected. `git diff --check`: exit 0.

## Uncalled helper retirement

The isolated lastSegment group and its eight fake-host/source cases were removed
with the uncalled generic helper. The nine-group/92-case table above is historical;
the maintained session target retains eight groups and84 source cases. Production
isGrvtRead still uses the shared transport predicate, which owns its path split.

Retirement checks: retained production API prefix is byte-identical, and the
eight retained live-source groups/84 cases equal the original oracle output.
Production exchange_binding29 and session_routing8 pass, including actual private
orders/fills/positions and balance UPNL. Scoped parity7459rows/538items passes.
Native/wasm strict scoped Clippy, all-features cancellation6pass/1exportignore,
and fmt/diff checks pass. No retained function body or cancellation pin changed.
