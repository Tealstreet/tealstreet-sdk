Moved **6 pending → ported, 0 pending → divergent** from integration
`128daa572bff854c64297a24e95ac320b7f78f60`. The measured venue triple is **19 / 10 / 112**
(ported / divergent / pending), versus **13 / 10 / 118** at that exact base.
The specified base had 118 pending rows, rather than the 122 quoted in the
assignment. Python 3.14 `tomllib` counted the seven venue manifests, comparing
symbol identities/statuses against `git show` of the specified base.
This census is not proof of all 141 venue rows.

All final gates below were measured at **`6cc3c06d62ba3f1b8ca9075df7db6a0869f93d8f`**.
The tested commit's immediate parent is `fdbabeaac7bf515043e31db1f1505ee99a7c5147`;
the slice's first implementation commit branches directly from the integration
SHA above. No PR 7401 implementation ancestry was carried into this branch.
The following evidence-only commit is not claimed as a separately tested SHA.

The six newly ported source bodies are:

- `PolymarketExchange.get name` → `safe-cex-venue-polymarket/src/spot_contracts.rs::name`.
- `PolymarketExchange.getPositionMode` → `safe-cex-venue-polymarket/src/spot_contracts.rs::get_position_mode`.
- `PolymarketExchange.setLeverage` → `safe-cex-venue-polymarket/src/spot_contracts.rs::set_leverage`.
- `PolymarketExchange.changePositionMode` → `safe-cex-venue-polymarket/src/spot_contracts.rs::change_position_mode`.
- `PolymarketExchange.changeMarginType` → `safe-cex-venue-polymarket/src/spot_contracts.rs::change_margin_type`.
- `PolymarketExchange.fetchSymbolMaxLeverage` → `safe-cex-venue-polymarket/src/spot_contracts.rs::fetch_symbol_max_leverage`.

The constant returns and warning text/severity match source behavior. An injected
synchronous fallible logging callback binds to shared BaseExchange::log. The
three warning bodies invoke it at call time and return Ready<Result<(), E>>;
original logging failures retain their opaque identity. Receiver projection is
declared with exact Rust targets and has no identified behavioral divergence in
these method bodies. Class, constructor, stateful lifecycle, store, HTTP and
sockets remain separate pending rows.

| Gate at the tested SHA | Exit | Scope/result |
| --- | --- | --- |
| Native controls | 0 | Baseline 0; seven named faults 101; seven restorations 0 |
| Cargo check | 0 | safe-cex-venue-polymarket, all targets, jobs 2, locked |
| Venue tests | 0 | 11 tests passed; unit/doctest targets contained zero tests |
| Formatting | 0 | Only safe-cex-venue-polymarket |
| Scoped mapping | 0 | 141 rows, 36 public Rust items, no exemptions |
| Extractor check | 0 | Output read: Would update 0 manifests; 0 added, 0 orphaned symbols |

The extractor enumerated 7341 symbols across 512 files; this is discovery, not a
behavioral proof of that population. All Cargo invocations were serial. The
manifest provenance array was produced by the extractor's serializer from mapped
Rust targets, followed by parity:extract; no TOML array was hand-spelled.

| Fixed seven-control floor (stopped at eager-prefix) | Named assertion | Fault exit | Restored exit |
| --- | --- | --- | --- |
| venue-name | `spot_name_native` | 101 | 0 |
| position-mode | `spot_mode_native` | 101 | 0 |
| max-leverage | `spot_leverage_native` | 101 | 0 |
| leverage-warning | `spot_log_native_message:setLeverage` | 101 | 0 |
| position-warning | `spot_log_native_message:changePositionMode` | 101 | 0 |
| margin-warning | `spot_log_native_message:changeMarginType` | 101 | 0 |
| eager-prefix | `spot_warning_native_eager:setLeverage` | 101 | 0 |

No fault survived this final seven-control floor. The runner refuses dirty lane
files before binding to HEAD and restores production source in finally.

**Corrected finding:** the initial implementation at
`fdbabeaac7bf515043e31db1f1505ee99a7c5147` used lazy async bodies at
`spot_contracts::set_leverage`, `change_position_mode`, `change_margin_type`.
Its await-only oracle passed while missing the source synchronous logging prefix.
A new before-poll supplement, applied as an explicitly recorded uncommitted test
variant against that production SHA, failed at
`spot_warning_native_eager:setLeverage`, exit 101; the original oracle still
passed in the same run. Thus the earlier passing gates do not prove call-time
logging. The final implementation fixes all three bodies. Final control seven
reinstalls deferred setLeverage and measures the same named exit-101 failure,
followed by exit-0 restoration at the tested SHA above.

The source oracle executes the exact six original members and actual shared
PositionMode/LogSeverity enum modules once per Node process. Its fixed floor
stops after six methods × five inputs (indices 0–4): 30 native return comparisons,
15 native warning comparisons and three full logger rejection comparisons.
Source immediateCalls is captured before await for both success and failure.
The supplemental Rust test checks all three warning call sites before any future
poll and requires exactly one call afterward. Number inputs/outputs are captured
as binary64 bits before JSON serialization. Native enums, f64 bits, warning
strings/severity and Rc error identity/fields are asserted directly; there is no
response decode/re-encode comparison. Capture: arm64, Node major 24.

A sub-agent reviewed implementation, actual TS methods/enums, oracle, native
assertions, controls and manifest mappings. Its initial review also missed the
timing gap; it reviewed the correction and found no remaining actionable issue.
The reviewer ran no gates. Final measured proof comes from the commands above.

Not run: host/wasm Clippy, workspace compilation, monorepo suites, live or
credentialed venue calls, CI reruns, or inherited mutation matrices. No claim is
made about host Promise/microtask scheduling or the unimplemented exchange shell.

If PR 7401 lands first, the exchange manifest's rust_files metadata may need to
retain both its utilities.rs entry and this spot_contracts.rs entry. This branch
was intentionally created directly from the requested integration base.

Reproduce from repository root, with one Cargo invocation at a time:

```sh
export SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
export POLYMARKET_CONTROL_LOG_DIR=/tmp/polymarket-spot-controls
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/spot_contracts_controls.py
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --jobs 2 --locked
/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked --jobs 2 -- --nocapture
/opt/homebrew/opt/rustup/bin/cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --check
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Complete raw output, gate commands/exits, native control summaries and census
JSON are retained under ignored
`sdk-rs/target/polymarket-spot-contracts/<tested-sha>/`. Initial finding output
is retained under that same directory for the initial production SHA.
