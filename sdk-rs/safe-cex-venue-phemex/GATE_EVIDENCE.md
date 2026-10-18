# Phemex configuration slice evidence

Integration base: `a3d514bec82d6760d4e9e7a27186f992d175c36d`, merged down
at `8f89e4c9fe01342ff4fbb07ece829a91cec7e840`.

Python 3.11 `tomllib` parsed all five Phemex manifests: initial
ported/divergent/pending = 0/0/79; this slice = 12/0/67. No rows are
not-applicable. The two response-type rows, API (including BROKER_ID), exchange
methods, and websocket rows remain pending. This is not an operational venue.
All five Phemex TypeScript files were read (2,857 lines at the merge state).

## Executed witnesses

`constants_match_actual_typescript` executes the checked-in phemex.types.ts,
shared types.ts and LogSeverity.ts through the installed TypeScript compiler.
It compares 12 runtime exports and 144 property-read observations. Table entry
order and endpoint-array duplicates are compared directly. RECV_WINDOW compares
pre-serialization typeof and binary64 bits. Inherited function/object observations
are tagged before serialization; there is no JSON-only function comparison.
There are no captured shared fixtures or response-model roundtrips in this slice.

`tests/constants_controls.py` ran fourteen sequential production mutations (one for each of the twelve ported constant rows, plus two inherited-read controls). Each
failed at the named assertion below, restored constants.rs in a finally block,
and then passed `constants_match_actual_typescript`:

| Mutation | Assertion |
| --- | --- |
| BASE_URL livenet host changes | ordered constant BASE_URL |
| BASE_WS_URL public key changes | ordered constant BASE_WS_URL |
| ENDPOINTS market path changes | ordered constant ENDPOINTS |
| Periodic endpoint switches to markets | ordered constant GRAFANA_ENDPOINTS_PERIODIC |
| Every-call endpoint switches to markets | ordered constant GRAFANA_ENDPOINTS_EVERY |
| Public endpoint switches to batch orders | ordered constant PUBLIC_ENDPOINTS |
| New status maps to Canceled | ordered constant ORDER_STATUS |
| Market type maps to Limit | ordered constant ORDER_TYPE |
| Long side maps to Short | ordered constant POSITION_SIDE |
| GoodTillCancel maps to PostOnly | ordered constant TIME_IN_FORCE |
| BUY maps to Sell | ordered constant ORDER_SIDE |
| 20000 changes to 20001 | RECV_WINDOW bits |
| All inherited lookups become missing | table lookup BASE_URL.constructor |
| Only inherited prototype object becomes missing | table lookup BASE_URL.__proto__ |

The sub-agent found that an ordinary observation dictionary silently consumed
`__proto__` through its setter. Null-prototype dictionaries and own-key/count
assertions on both sides repair that false green; the prototype-only mutation proves the
repaired observation is consumed. The source index also caught the exported
lookup alias missing from the manifest; the ORDER_STATUS split mapping now names
that operation and explains its reuse. No public-item exemptions are used.

## Commands

Run from this worktree root; cargo is `/opt/homebrew/opt/rustup/bin/cargo`.
The oracle environment is:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-phemex --jobs 1`:
  1 integration test passed, 0 failed; 0 unit tests and 0 doctests.
- `python3.11 sdk-rs/safe-cex-venue-phemex/tests/constants_controls.py`:
  14 named mutation failures, 14 restored passes.
- `CARGO_TARGET_DIR="$PWD/sdk-rs/target" cargo run --locked --manifest-path sdk-rs/safe-cex-venue-phemex/tests/manifest-gate/Cargo.toml --jobs 1`:
  79 manifest rows, 13 public items, 0 errors. Shared source resolution precedes
  filtering to Phemex; other owners' rows are excluded.
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-phemex --all-targets --locked --jobs 1 -- -D warnings`:
  passed on aarch64-apple-darwin.

- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-phemex --target wasm32-unknown-unknown --lib --locked --jobs 1 -- -D warnings`:
  passed separately from host lint.
- `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-phemex --check`
  and the isolated manifest-tool format check both passed.

- `CARGO_TARGET_DIR="$PWD/sdk-rs/target" cargo clippy --locked --manifest-path sdk-rs/safe-cex-venue-phemex/tests/manifest-gate/Cargo.toml --jobs 1 -- -D warnings`:
  passed for the isolated test tool.

No monorepo test run, real exchange call, REST/write/websocket acceptance test,
or response numeric-ingress witness is claimed. The sole numeric constant is
checked before serialization; typed response ingress remains pending.

The control runner parses its manifest with Python tomllib and asserts exact
coverage of ported constant-row names before starting any Cargo invocation.
Each invocation runs only the constants test target; Node executes once for the
whole 12-export/144-lookup corpus, never once per lookup. Cargo runs sequentially.


## Response types and broker identifier supplement

This supplement preserves the earlier constants evidence; it does not claim the
new corpus existed during the first slice. The earlier twelve declarations and
144 lookup cases are unchanged. BROKER_ID adds one actual-source declaration to
the constants corpus and its own production mutation, failing
`ordered constant BROKER_ID`, followed by a restored pass.

`types_match_live_typescript_fields_and_numeric_ingress` executes one Node process
for the full corpus. The actual TypeScript compiler resolves the source aliases:
7 account fields, 42 position fields, and the generic intersection. The numeric
observations are generated typed projections over JSON.parse, not a claim that an
erased type alias has a runtime decoder. Profile: Node v26.7.0, major 26, arm64,
Darwin. NaN capture canonicalizes only NaN; signed zero remains exact.

Measured corpus: 71 response cases and 74 envelopes, each checked through text,
owned Value and borrowed Value ingress (435 typed decodes). Every one asserts
native decoded fields; no roundtrip is the sole field-identity proof. All numeric
payload edges also traverse the envelope's flatten buffer. Envelope metadata uses
67.5, distinct from payload sentinels, plus three metadata numeric-edge cases.

Executed type controls, each failing the assertion and passing after restoration:

| Production/control mutation | Named assertion |
| --- | --- |
| Account balance/used serde names swapped symmetrically | native account.accountBalanceRv |
| Position entry-price serde names swapped symmetrically | native positions[0].avgEntryPriceRp |
| Envelope code skipped during decoding | envelope text ingress |
| Compiler census drops inherited required property | inherited required-field census |

The final row status for both types is CHOSEN divergent, not full-domain parity.
The manifest notes disclose UTF-8/recursion boundaries, projection of known fields,
and the envelope's unsupported overlapping-metadata payload precondition. The
required-overlap witness is not a universal disjointness claim. No REST, history
capability, or websocket row changes in this slice.

Slice readiness gates: scoped constants + types targets passed (2 tests), strict
host all-targets clippy passed, strict wasm32 lib clippy passed separately, and
fmt --check passed. The scoped official manifest gate measured 79 rows / 18 public
items / 0 errors. Python tomllib measured 13 ported / 2 divergent / 64 pending.

## Local calculatePriceDiff utility increment

This increment is local after reviewed/pushed constants/types SHA
23e646a1c6c8e3730a6606899cdfc8d4483f2b40. The earlier full gate sheet does
not cover this utility increment. Python 3.14 tomllib: 14 ported / 2 divergent /
63 pending. One scoped utilities test passed with 722 cases from one Node process
(Node v26.7.0, major 26, arm64, darwin). Actual AST-selected calculatePriceDiff
executes with actual safe-math and PositionSide source modules. NaN is the only
canonicalized result; signed zero remains exact. Production branch inversion
failed `price difference native bits`, then restored and passed. Format/diff checks
passed. Scoped manifest gate: 79 rows, 19 public items, 0 errors after correcting
the rename note to the exact Rust target. No full readiness sheet claimed here.

## Utility slice after integration dec68e5490cb

Python 3.14 tomllib: 14 ported / 4 divergent / 61 pending. All three utility
bodies are implemented; the two symbol helpers declare CHOSEN UTF-8/JSON domains.
Live actual-TypeScript corpus uses one Node process: 722 price-difference cases,
1,112,064 Unicode scalars (1,580 changed uppercase mappings), 8 symbol inputs,
26 symbol outputs, and 6 raw numeric-output lexemes. Capture header records
Node v26.7.0 / major 26 / arm64 / darwin. Only NaN is canonicalized; signed zero
remains exact. Source lone-surrogate behavior is asserted as the excluded UTF-8
boundary. Market numeric info is decoded through shared ingress.

Four production mutants failed named assertions and each restored to a passing
scoped utility test: `price difference native bits`, `native symbol uppercase`,
`native symbol output first match`, and `native symbol output numeric type`.
The fourth control closes the review finding that Number coercion could hide a
Number-to-string error. Tests now assert native numeric type before exact bits.
This paragraph records these precommit controls; the owner reports the subsequent
full scoped readiness results against the exact committed SHA separately.

## getAccount REST read slice after fetched integration 710e663156c8

Python 3.14 tomllib: 14 ported / 5 divergent / 60 pending. getAccount now has
an implementation with explicit configured xhr-read and fallible-emitter seams;
createAPI and the remaining REST methods are pending. The new row declares its
JSON/UTF-8 and future-lifetime choices rather than claiming full JS object or
abandoned-Promise equivalence. No Store mutation site is added.

Actual AST-selected method witness: 30 cases, 17 successes, 13 failures, one Node
process for the corpus. Header: Node v26.7.0, major 26, arm64, darwin. Native
user_id assertions precede any serialization; missing/null chains, falsy IDs,
Number lexical edges, arrays/objects, conversion errors, and read/emitter rejection
identity are compared. Source requests and emitter effects are ordered records.
Existing Phemex symbol-scoped/no-income-ledger capability finding is cited in
CLAUDE.md; this slice does not re-derive or change the fills-axis capability.

Precommit scoped account test and cargo check passed. Three production controls
compiled and failed `account request and emit order`, `native account userId`,
and `account error name`, respectively; each restoration passed. Extractor
serialization matches for the changed manifest. Subsequent full scoped readiness
results are reported separately against the exact committed SHA.

## Raw ticker REST read after integration e53d89eddeef

This work is on feat/rust-venue-phemex-rest-reads, based on the pinned account
slice plus the merged MEXC propagation repair. PR 7266 remains at a55c6c4295.
The prior Phemex gates excluded MEXC and never claimed workspace wasm success.

Python 3.14 tomllib: 14 ported / 6 divergent / 59 pending. The new raw ticker
row is CHOSEN for JSON/UTF-8, reference identity and host-owned future lifetime.
Its actual method plus actual get-key and Lodash dependencies execute 23 cases
in one Node process: 13 fulfilled, 10 rejected. Header records Node v26.7.0,
major 26, arm64, darwin. Result capture checks native types and binary64 bits,
canonicalizing NaN only, retaining signed zero. Successful raw payloads also
retain their original numeric lexemes. Error and emitter identity are checked
before output projection. Source missing-error behavior remains a named ts-bug.

Precommit scoped ticker_reads test passed. Three production mutations compiled
and exited 101 at `native ticker result`, `ticker request and emit order`, and
`ticker outcome`; each restored and passed. Subsequent full scoped readiness
results are reported against the committed SHA, not inferred from this paragraph.

## Fourth branch: audit selectors and validateAccount

Base fetched directly: dbdf3d15dddc29d9ae885398c62fd0fe2a25acbc. The pre-row audit
is committed separately at 0e5e98e148854c27e9c11e42831a0e30a0ad6a94, sub-agent
review zero findings. Census: zero assertion-bearing gated closures, two control
runner selections without floors, and one adjacent gated ticker assertion block.
The ticker predicate-disabled mutant compiled and passed its original target;
the repair pins two executions and fails that same mutant by name before a
restored pass. Twelve in-process runner controls fail named selection/execution
assertions; no Cargo/Node subprocesses are used for those runner controls.

validateAccount reuses shared Balance/ValidateAccountResult/AccountValidationError
and numeric helpers. Python 3.11 tomllib: 14 ported / 7 divergent / 58 pending.
The row explicitly declares JSON/UTF-8, property/alias identity and future-lifetime
choices. API construction remains pending. The existing symbol-scoped/no-income-
ledger capability finding remains cited in CLAUDE.md, not re-derived.

The actual AST-selected validateAccount initializer executes 62 cases in one Node
process (Node26.7.0, major26, arm64, darwin). Native expectations independently pin
23 balances, 37 absent balances, two emitter rejections, 92 direct balance-field
comparisons, 62 read calls and 24 emitter calls. Source/native event vectors and
all returned error values are compared whole. Nonempty object and array errors
supplement the empty truthy-value cases so truncation cannot hide in empty input.
Number bits canonicalize NaN only and retain signed zero.

Twelve production controls cover total-field swap, free reversal, nonfinite-total
admission, used negative-zero normalization, returned msg/message swap, swallowed
emitter failure, plus empty/subset/extra output independently for object and array
error values. They require compiled execution, exit101 and a named native
assertion; exact sources restore in finally blocks and each restored target must
pass. Exact-SHA readiness gates are reported separately after commit.

## Session and position control

Base: `74acc315ebdfc83947289f66520380daafca28e2`.
Scoped-index gate fix: `bfc2027a9d30523b190d720165a03571c409e7df`.
Implementation SHA: `c0fc410d1637b264cae051a3a29c3fec22aab2e4`.
Merge-down SHA: `54a87fc99333e61a0e637c0e5696984d2ad0d51a`.
Integration parent: `95b56a79bc646156e1812b17428e0fb6295413a5`.
The subsequent evidence commit changes only this sheet.

Tomllib measures **9 pending→ported, 0 pending→divergent**.
Before: **14 ported / 9 divergent / 56 pending**. After:
**23 ported / 9 divergent / 47 pending**, 79 rows.

| Source member | Rust function |
| --- | --- |
| `dispose` | `session_control::dispose` |
| `_startPrivate` | `session_control::start_private` |
| `_startPublicDataOnly` | `session_control::start_public_only` |
| `tick` | `session_control::tick` |
| `listenOHLCV` | `session_control::listen_ohlcv` |
| `listenOrderBook` | `session_control::listen_order_book` |
| `fetchPositionForSymbol` | `position_control::fetch_position_for_symbol` |
| `fetchRawPositionModeForSymbol` | `position_control::fetch_raw_position_mode` |
| `getOrderPositionSide` | `position_control::get_order_position_side` |

Production inputs/payloads/receivers/callbacks/mapper targets/listeners/errors
are generic opaque host values. These functions do not convert them through
JSON or shared model structs. Rust owns branch, call, await, loop, capture and
error scope decisions; host projections implement original external members,
primitive property operations and inline object construction. The source's
Binance public-watch log text is retained. Native zero-balance values and loaded
flags originate in production. No existing shared type or trait changed.

`tests/session-position-oracle.cjs` executes the actual nine TypeScript class
members, including lexical `super.dispose`, with controlled base/store/socket,
transport, scheduler, mapper and clock-independent member projections. It loads
the actual shared enums and Phemex endpoint constants. Native tests compare
**95 cases in nine tests**, field presence, UTF-16 strings, binary64 observations
and effect order before encoding. Separate native assertions check original
balance/position/loaded/options/callback/receiver/handler/mapper/listener/error
identities, separately allocated empty arrays and actual Pending continuations.

An audit fixed two observable details before commit: mapping retains one bound
`mapPosition` target before reading map length, even if the current member is
replaced; the second `info.posSide` access is required after a truthy optional
first read. Source cases exercise mapper replacement, a binding getter failure
on an empty list, changed/disappearing info getters, live options/symbol changes
after suspension, sparse-slot host projections and malformed null position rows.
(The sparse-slot contract is implemented; the corpus exercises dense arrays.)

| Gate | Exit and result |
| --- | --- |
| Scoped Phemex all-targets check, jobs=2 | 0, before and after merge |
| `cargo test ... --test session_position -- --nocapture --test-threads=1` | 0; 9 tests, 95 cases |
| Actual extractor writer after manifest edits | 0; 7341 symbols, 512 files, 0 manifests changed |
| Post-merge extractor freshness | 0; 0 manifests would change |
| Actual scoped Rust indexer and status gate | 0; 79 rows, 106 public items, no exemptions/errors |
| Tomllib transitions and declared target files | 0; all nine rows have real indexed targets |
| `git diff --check` | 0 |
| Standalone merge byte-preservation check | 0 |

`tests/session-position-controls.py` records **11 compiled mutations**, each
failing a named decoded-native assertion with **101**, then passing after
restoration with **0**:

| Mutation | Named assertion |
| --- | --- |
| Dispose private before public | PH native effect socketDispose |
| Private isHedged true→false | PH native effect setting |
| Public used balance 0→1 | PH native effect update |
| Tick loaded positions→orders | PH native effect update |
| OHLCV selects book method | PH native effect handler |
| Book selects OHLCV method | PH native effect handler |
| USDC position request uses USDT | PH native effect get |
| Invert raw-mode symbol match | PH native effect formatIn |
| Reverse buy-side selection | PH native result |
| Rebind mapper per row | PH native effect bind |
| Make required second info read optional | PH native optional then required info path |

The initial nine controls passed. After the position audit, affected position,
mode and side controls were rerun and the mapper/info controls added. Unchanged
session/delegation controls were retained; their source decisions and scoped
case families did not change. Generated logs and JSON remain ignored under
`sdk-rs/target/phemex-session-position-evidence/`.

The existing manifest gate initially failed before reaching Phemex: its full
workspace index encountered Webworker's unsupported `thread_local!` macro.
The gate now parses the real Cargo workspace configuration, replaces only its
member list with the actual Phemex crate path, writes that input under ignored
`target/`, and runs the same `safe_cex_parity::rust_index` and `check` APIs.
All Phemex public targets and manifests are indexed; no item/status exemption
was added. External aliases retain the actual indexer's supported alias policy
and the scoped compiler check proves dependency resolution. The gate-only TOML
dependency uses the existing locked version; no dependency version was updated.

Merge-down preserved Phemex source/tests/dependencies/manifests/TypeScript and
shared numeric/type/indexer sources byte-for-byte against the implementation
SHA, so native and mutation exits stand. A post-merge scoped check passed.
Not run: full crate/workspace suites, workspace-wide check, live exchange calls,
credentials, browser automation, CI waiting, or PR creation/merge.


## REST account settings, cancellation and market lookup

Base: `0e116b60e90e37e08f3995645493fc20176c0b86`.
Implementation/gate SHA: `9f8b3ed481a7811e46aaefe3c66b354143cea1d0`.
Merge-down SHA: `64a9268f655ec2a094f9d83bcecc4099784a02f3`.
Integration parent: `74fc4846d408b2b29e843728c4233fe900abb371`.
Branch: `feat/rust-phemex-rest-account-orders`.

Measured with Python tomllib against the base: **8 pending -> ported,
0 -> divergent**; **23 ported / 9 divergent / 47 pending** becomes
**31 ported / 9 divergent / 39 pending**, 79 total rows.

| Source row | Rust function | Actual-source cases |
| --- | --- | --- |
| resolveLeverageContext | rest_account_control::resolve_leverage_context | 18 |
| changePositionMode | rest_account_control::change_position_mode | 11 |
| changeMarginType | rest_account_control::change_margin_type | 8 |
| setLeverage | rest_account_control::set_leverage | 4 |
| setLeverageHelper | rest_account_control::set_leverage_helper | 21 |
| cancelOrders | rest_order_control::cancel_orders | 13 |
| cancelSymbolOrders | rest_order_control::cancel_symbol_orders | 4 |
| getOrFetchMarketForSymbol | rest_order_control::get_or_fetch_market | 9 |

Both modules are under `safe-cex-venue-phemex/src/`; manifests declare every
new target and associated host trait/member. The actual scoped indexer checks
all 79 rows against 220 public items, with default empty exemptions and no
errors. A separate tomllib check confirms all 40 off-pending rows have nonempty
targets in declared, existing Rust files.

Rust owns control order, fallback choice, numeric signs/clamping, cancellation
parent IDs, grouping, chunking, response decisions, awaits and catch scopes.
Opaque host types preserve symbols, markets, rows, contexts, replies and errors;
host projections implement the source's shared/member/store operations.
Production uses no JSON model conversion. The actual eight TypeScript members,
Phemex constants, shared enums/get-key and installed lodash/p-iteration execute
in `tests/rest-account-order-oracle.cjs`.

`tests/rest_account_orders.rs` compares decoded native results and individual
effects before witness encoding. It observes binary64 bits (including NaN,
infinities and signed zero), full UTF-16 values and object keys, original
object/error identities, cache fallbacks and original input status IDs.
It drives real Pending continuations for mutable bounds, cancellation input
changes and market cache replacement. Formatter mutations replace GET/PUT/DELETE
members after the source captured its call target; the native path retains that
target. Case floors are asserted per family, totaling **88 cases / 8 tests**.
The cancellation corpus uses dense ordinary order arrays and ordinary response
DTOs; sparse slots and opaque inner/outer flattening remain host contracts,
not claims of additional tested fixture coverage. Dynamic prototype/getter
instrumentation is not an exhaustive claim of this corpus.

| Gate | Exit/result |
| --- | --- |
| Scoped Phemex all-targets cargo check, jobs 2 | 0 |
| Scoped rest_account_orders native tests, test threads 1 | 0; 8 tests, 88 cases |
| Extractor writer after manifest edits | 0; 0 manifests changed |
| Actual scoped Rust indexer/status gate | 0; 79 rows, 220 public items, no errors |
| Tomllib transition/file checks | 0; 8 ported transitions, all 40 targets declared |
| Post-merge scoped all-targets check | 0 |
| Post-merge extractor freshness | 0; 0 manifests would change |
| Standalone source/test byte-preservation diff | 0 |
| Extended dependency/source/indexer preservation diffs | 0 |
| git diff --check | 0 |

`tests/rest-account-order-controls.py` runs eight production mutations, one
per new row. Each compiled and failed the named native assertion with **101**,
then passed after restoration with **0**:

| Mutation | Named assertion |
| --- | --- |
| Reverse raw leverage margin sign | REST native result |
| Hedged position target -> OneWay | REST native effect put |
| Cross margin unary negation -> positive | REST native effect helper |
| Omitted helper margin override -> true | REST native effect helper |
| Negative prepared leverage -> positive | REST native effect put |
| Cancellation batch 10 -> 9 | REST native effect delete |
| Omit cancelSymbolOrders await | REST native complete effects |
| Market loaded flag true -> false | REST native effect update |

Generated control logs/JSON stay ignored under
`sdk-rs/target/phemex-rest-account-order-evidence/`; this manual tracked sheet
records the verified exits. Source was restored before the implementation commit.

Merge-down preserved the gated Phemex source/tests, Cargo configuration/lock,
Phemex manifests and TypeScript, shared get-key/types/log severity, numeric/type
crates, indexer source, extractor and yarn lock byte-for-byte. The standalone
`git diff --exit-code 9f8b3ed481a7811e46aaefe3c66b354143cea1d0 HEAD --
sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests` returned 0.
The package script change only extends Kucoin's oracle target list; the Phemex
oracle and parity commands are unchanged. Native and control gate exits stand.

Not run: full crate/workspace test suites, workspace-wide check, live exchange
calls, credentials, browser automation, CI waiting, or PR creation/merge.

Pre-push fetch required a second merge-down: `9cbed2b1c26d923ba0cd3147de39a97d20402e24`, integration
parent `7495cc9eec52fd1c6444775455859a45e20ad247`. It changed only Lighter files.
The standalone Phemex source/test diff and the extended preservation diff
were rerun against the implementation SHA; both returned 0. All prior native,
control, indexer, compiler and extractor exits stand on these unchanged inputs.


## Public websocket setup and frame routing

Base: `7078883cf0aa0de5f11f253c7cee83d98be36265`.
Implementation/gate SHA: `257223dd4e17d05e7788ae1b9cc62acf0f34cfe2`.
Merge-down SHA: `85d74d8f3b8b1913cf59d40cba5ccfc2c8e477a6`.
Integration parent: `c0f4ba828cc10f574e34896c955784ca383909aa`.
Branch: `feat/rust-phemex-public-routing`.

Tomllib comparison against the base: **8 pending -> ported, 0 -> divergent**.
Phemex moves **31 ported / 9 divergent / 39 pending** to
**39 ported / 9 divergent / 31 pending**, 79 total rows.
The class and remaining ticker/listener/orderbook processing rows stay pending.

| Phemex public websocket source row | Rust function | Source cases |
| --- | --- | --- |
| connectAndSubscribe | connect_and_subscribe | 5 |
| onOpen | on_open | 5 |
| subscribe | subscribe | 7 |
| updateSubscriptionLimitWarning | update_subscription_limit_warning | 8 |
| ping | ping | 6 |
| dispatchTopic | dispatch_topic | 8 |
| getKlineInterval | get_kline_interval | 8 |
| onMessage | on_message | 15 |

All targets are in `safe-cex-venue-phemex/src/public_routing.rs`. The manifest
also covers Request, Parameter and every RoutingHost associated type/method.
The real scoped indexer checks **79 rows / 285 public Rust items / 0 errors**,
with default empty exemptions. A separate tomllib check verifies declared,
existing Rust files for all **48 off-pending rows**.

Rust owns disposal guards, network/listener order, request IDs, UTF-16 topic
splitting, kline parameter selection, warning thresholds/latching, ping packet
construction, callback statistics/iteration, interval cache setup and frame
routing precedence/deduplication. Host contracts project original transports,
serialization/coercion, object/map members, iterator handles and callback calls.
Production retains opaque frame/callback/error carriers without JSON conversion.

`tests/public-routing-oracle.cjs` executes the eight actual TypeScript members
and actual static threshold fields, Phemex constants, shared enums/log severity,
and actual shared jsonParse. `tests/public_routing.rs` observes native UTF-16
strings and binary64 bits before witness encoding, compares each effect plus
complete observations, and enforces independent per-family case floors:
**62 source cases / 8 tests**. Native assertions also retain original error and
callback payload identities. The corpus covers disposed/testnet cases, listener
and send failures, captured send targets when serialization replaces a member,
integer-key subscription order, missing/NaN/infinite resolutions, lone-surrogate
topics, warning boundaries/reset/latching, negative-zero/infinite/large ping IDs,
live callback append/removal and captured-array replacement, cache reuse and
captured timeframes during cache assignment, invalid parsing, pong precedence,
string-symbol guards, kline deduplication and empty-kline precedence over books.
This is ordinary frame/host fixture coverage, not exhaustive prototype/getter,
custom coercion, timer, listener teardown or orderbook snapshot coverage.

| Gate | Exit/result |
| --- | --- |
| Scoped Phemex all-targets cargo check, jobs 2 | 0 |
| Scoped public_routing native tests, test threads 1 | 0; 8 tests, 62 cases |
| Extractor writer after manifest edits | 0; 1 manifest normalized, 0 added/orphaned |
| Actual scoped Rust indexer/status gate | 0; 79 rows, 285 public items, no errors |
| Tomllib transitions and target files after writer | 0; 8 ported transitions, 48 off-pending rows |
| Standalone merged source/test byte-preservation diff | 0 |
| Extended source/configuration/indexer preservation diff | 0 |
| Lockfile package-record comparison | 0; only Bitget package changed |
| Post-merge scoped all-targets cargo check | 0 |
| Post-merge extractor freshness | 0; 0 manifests would change |
| git diff --check | 0 |

`tests/public-routing-controls.py` establishes one compiled production mutation
per new row family. All eight fail a named decoded-native assertion with **101**,
then pass after source restoration with **0**:

| Mutation | Named assertion |
| --- | --- |
| Invert websocket testnet selection | PUBLIC native effect create |
| Packed subscription id 1 -> 2 | PUBLIC native effect serialize |
| Replay subscription id 10 -> 11 | PUBLIC native effect serialize |
| Warning threshold 18 -> 17 | PUBLIC native complete observation |
| Ping wire id -> id + 1 | PUBLIC native effect send |
| Reverse once-per-topic stats condition | PUBLIC native effect callback:a |
| Interval cache key -> numeric + 1 | PUBLIC native complete observation |
| Orderbook routing prefix -> wrong prefix | PUBLIC native effect dispatch |

The first development run caught an incomplete ping wire template; it was
corrected before the positive and mutation gates above. The interval audit
added the captured-timeframes case and its implementation before those gates.
Generated logs and JSON remain ignored under
`sdk-rs/target/phemex-public-routing-evidence/`; this manual tracked sheet is
append-only and preserves all earlier Phemex evidence bytes.

The standalone `git diff --exit-code 257223dd4e17d05e7788ae1b9cc62acf0f34cfe2
HEAD -- sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests`
returned 0. Extended comparisons preserve Phemex Cargo/manifests/TypeScript,
shared types/log severity/parser/numeric sources, Rust indexer, extractor,
package scripts and yarn lock. Cargo.lock adds an existing crypto dependency
only to Bitget; tomllib package-record comparison verifies every other package
record, including all Phemex dependencies, is unchanged. A post-merge scoped
compiler check passed. Native/mutation/indexer exits stand on unchanged inputs.

Not run: full crate/workspace suites, workspace-wide check, live exchange calls,
credentials, browser automation, CI waiting, or PR creation/merge.


## Private websocket session and API signing control

Base: `7e9e5c660472d985e231394357b25ed171b929f2`.
Implementation/gate SHA: `af6995e83e30967001edaedf68d01c4a65a9d241`.
Merge-down SHA: `f4d0574a014f50dbe645181da74ed12a5ac78200`.
Integration parent: `fc23eb289f39c11f1bf30b56f1848254b2a520ac`.
Branch: `feat/rust-phemex-private-session`.

Tomllib measurement against base: **7 pending -> ported, 1 -> divergent**.
Phemex **39 ported / 9 divergent / 31 pending** becomes
**46 ported / 10 divergent / 23 pending**, 79 total rows.

| Source row | Rust function | Status | Source cases |
| --- | --- | --- | --- |
| BinancePrivateWebsocket.dispose | private_session::dispose | ported | 6 |
| schedulePositionsAndBalanceSync | private_session::schedule_positions_and_balance_sync | ported | 7 |
| connectAndSubscribe | private_session::connect_and_subscribe | ported | 5 |
| onOpen | private_session::on_open | ported | 4 |
| ping | private_session::ping | ported | 6 |
| auth | private_session::auth | ported | 14 |
| syncPositionsAndBalance | private_session::sync_positions_and_balance | divergent | 11 |
| createAPI | api_transport::create_api / sign_request | ported | 16 |

Both modules are under `safe-cex-venue-phemex/src/`. Manifests cover every
SessionHost/AuthHost/SyncHost/ApiHost/RequestHost public member and both API
functions. Actual scoped rust_index/check verifies **79 rows / 403 public items /
0 errors**, using default empty exemptions. Separate tomllib checks confirm
nonempty targets in declared, existing files for all **56 off-pending rows**.

The reconciliation row is **CORRECTED**: TypeScript's catch silently fulfills
account, fetch-construction or store-update failures; Rust returns the original
error to the caller. Tests explicitly compare source undefined fulfillment
against native original-error identity before comparing the remaining effects
and state. Successful reconciliation and best-effort allSettled handling of
individual symbol rejections preserve source order and behavior.

Rust owns timer/symbol state order, default symbols, private URL/listener setup,
unawaited auth start before ping, ping wire IDs, expiry arithmetic, remote/local
auth branch decisions, post-signing key rereads, concurrent symbol polling,
ordered allSettled results, symbol-ID replacement/first-ID deduplication and
loaded flags. API control owns network/timeout/retry/tunnel/interceptor order,
shallow-copy guards, params/data/expiry/url signing sequence and header output.
Opaque host contracts preserve original references, callbacks, options,
configurations, transport operations, shared coercion and serialization.
Production performs no JSON model conversion. Rust crypto is added only as a
dev dependency for real native HMAC assertions; its version was already locked.

`tests/private-transport-oracle.cjs` executes actual TypeScript members, the
actual createAPI body, constants/enums, safe-math/isNotNumber, retryReadsOnly,
create-hmac, qs and AxiosHeaders. Timers/transports/clock/signer are deterministic
hosts. `tests/private_transports.rs` enforces independent case floors and checks
native UTF-16/binary64 results before witness encoding: **69 cases / 8 tests**.
Coverage includes truthy/falsy timers, undefined second timer read, symbol
accumulation and debounce cancellation/fire, disposal guards, testnet listeners,
exact ping wires, remote errors/signature absence and live-key changes during
a real Pending await, actual HMACs with Unicode/lone-surrogate input, fractional
and nonfinite expiry, concurrent/reversed completion ordering, rejected symbol
reads, duplicate position IDs and original model/error/client identities,
public/remote credential guards, shallow copies, read-only retry predicate
outputs, nested query arrays, JSON bodies and constructed headers.

The development sync fixture initially completed native reads in the wrong
order. Its independent deferred reads now complete in reversed order while
preserving allSettled input order. The timer audit replaced an unwrap with an
opaque second read and added the undefined-read case. Final gates below include
both corrections and removal of an unused newly introduced host type slot.
The corpus is ordinary fixture coverage; exhaustive proxy/getter/custom
coercion behavior, real timer latency, HTTP execution, private frame handling,
and complete websocket class composition are not claims of this slice.

| Gate | Exit/result |
| --- | --- |
| Scoped Phemex all-targets cargo check, jobs 2 | 0 |
| Restored private_transports native tests, test threads 1 | 0; 8 tests, 69 cases |
| Extractor writer after final manifest changes | 0; 2 normalized, 0 added/orphaned |
| Scoped actual Rust indexer/status check | 0; 79 rows, 403 public items, no errors |
| Tomllib transitions and target file checks | 0; 7 ported / 1 divergent transitions, 56 off-pending targets |
| Standalone merged source/test byte-preservation diff | 0 |
| Extended relevant source/configuration preservation diffs | 0 |
| Cargo.lock package-record comparison | 0; only Blofin record changed |
| Post-merge scoped all-targets cargo check | 0 |
| Post-merge extractor freshness | 0; 0 manifests would change |
| git diff --check | 0 |

`tests/private-transport-controls.py` batches eight independent function
mutations into one compiled negative run. Each family test invokes its own
production function; auth/ping/session/sync families delegate other members to
their source-equivalent hosts, and API invokes only its factory/interceptor.
No mutated family function calls another mutated family function. The script
requires each named native panic, **all eight tests FAILED**, and compiled batch
exit **101**, then restores both source files and requires **8 passed / exit 0**.
This uses two builds per control run rather than sixteen. Final records:

| Mutation | Named decoded-native assertion |
| --- | --- |
| Dispose omits symbol clearing | PRIVATE native complete observation (dispose) |
| Debounce delay + 1 | PRIVATE native effect schedule |
| Invert private network choice | PRIVATE native effect create |
| Auth start replaced with ping | PRIVATE native effect ping (open) |
| Ping wire ID + 1 | PRIVATE native effect send (ping) |
| Auth wire ID 999 -> 998 | PRIVATE native effect send (auth) |
| Reconciliation positions-loaded true -> false | PRIVATE native effect update |
| API retry budget 3 -> 4 | PRIVATE native effect retry |

All eight final controls failed the named assertions in batch **101**, then
passed restored with **0**. Generated logs/JSON remain ignored under
`sdk-rs/target/phemex-private-transport-evidence/`; this manual sheet is tracked
and append-only, preserving all previous evidence bytes.

The standalone `git diff --exit-code af6995e83e30967001edaedf68d01c4a65a9d241
HEAD -- sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests`
returned 0. Extended comparisons preserve Phemex Cargo/manifests/TypeScript,
shared math/types/log severity/retry helpers and Rust numeric/crypto/store/
exchange/transport/indexer sources, extractor, package scripts and yarn lock.
Cargo.lock only changes Blofin to use existing base64 and crypto dependencies;
all Phemex dependency records are unchanged. A post-merge scoped compiler check
passes; native, control and indexer exits stand on unchanged gated inputs.

Not run: full crate/workspace test suites, workspace-wide check, live exchange
calls, real credentials, browser automation, CI waiting, or PR creation/merge.

Pre-push merge-down: `25f73edef387bf8c97126e81f8ecca43d91d9c3e`, integration
parent `0cd4f9259fd93ec1e6d1a574947320b48f376a04`. This adds Poseidon modules
to the shared crypto crate, so the earlier crypto byte-preservation claim
applies to the first merge SHA only. Existing crypto lib bytes, including HMAC
helpers, are an exact prefix of the new lib; only two new module exports are
appended. Every Phemex dependency lock record remains unchanged.

On this second merge, Phemex source/tests and all other extended relevant inputs
remain byte-identical to the implementation SHA (standalone diffs exit 0).
Affected gates were rerun: private_transports **0, 8 passed / 69 cases**;
scoped all-targets check **0**; extractor freshness **0, 0 manifests would change**.
Eight family mutation controls stand on unchanged function bodies and unchanged
existing HMAC helper bytes; the scoped Phemex indexer inputs are unchanged.

Last pre-push merge-down: `4041c176d5d6f1114e0d4470f66a189d1aa7b2b8`,
integration parent `600bbd302bdd09d20f938650e360385e68f55cba`. It adds Asterdex,
Backpack and Woo changes. Phemex source/tests are unchanged against the original
implementation SHA (standalone diff 0). Extended relevant inputs, including
crypto, are unchanged against the second merge (diff 0 excluding Cargo.lock).
Tomllib lock-record comparison finds only Backpack changed since that merge;
all Phemex dependency records remain unchanged. Compiler/native/control/indexer
exits stand. Extractor freshness rerun on this final merge is **0**, with
0 manifests that would change. Final measured Phemex triple: **46 / 10 / 23**.

## REST data: eight pending rows ported

Base: `c394ef75eec20388a2dbb80d9d69c0bf0fa69a4c`.
Implementation: `3942045e695c60fd7e48f1231eab9023e54550a4`.
Merge-down: `f30386c1433e53ee6052e5d6846a12ea43434607`, integration parent
`eb5339fff8f489456695778fe2af9701f8c18b69`.
Branch: `feat/rust-phemex-rest-data`.

Tomllib independently measures **54 ported / 10 divergent / 15 pending**, from
**46 / 10 / 23**: **8 pending -> ported, 0 pending -> divergent**. The eight
rows are fetchMarkets, mapPosition, fetchPositions, fetchPositionsAndBalance,
fetchOrders, mapOrderUpdate, fetchHistoricalFills and fetchOHLCV. Every one of
the 64 off-pending rows has nonempty Rust references in declared existing files;
the actual scoped Rust indexer validates those targets with default empty
exemptions: **79 rows, 466 public items, 0 errors**.

Rust owns the filters, request routing, concurrency, settlement validation,
balance aggregation, order children, fill parsing/deduplication and candle
request/mapping decisions. DataHost preserves opaque original values/errors,
UTF-16 strings, reference identity, leaf coercions and original external member
calls. Production does not serialize original carriers through JSON. The
TypeScript oracle executes the actual eight source bodies and actual shared
helpers, with UTC dayjs and a deterministic clock. Native assertions compare
decoded UTF-16 strings and binary64 values, each effect and complete results;
orders/cached markets and info references have native identity assertions.
Request futures produce real Pending states, including reversed currency
completion. Validation runs at each response settlement before ordered balance
processing. Numeric fill sides remain skipped, matching the source.

| Source family | Cases | Production mutation | Named native assertion |
| --- | ---: | --- | --- |
| Markets | 6 | Minimum leverage 1 -> 2 | RESTDATA native complete observation |
| Position | 9 | Notional multiplied by 2 | RESTDATA native complete observation |
| Positions | 6 | Flatten replaced with array | RESTDATA native complete observation |
| Orders | 2 | Orders reference replaced with positions | RESTDATA native orders identity |
| Order update | 11 | Amount + 1 | RESTDATA native complete observation |
| Balance | 9 | Aggregate free balance + 1 | RESTDATA native complete observation |
| Fills | 17 | Minimum limit 1 -> 2 | RESTDATA native effect get |
| Candles | 12 | Limit cap 1000 -> 999 | RESTDATA native effect get |

`tests/rest-data-controls.py` compiles all eight independent function mutations
in one negative program; each family calls its own mutated function and delegates
other source members to its host. It requires every named native assertion,
**8 failed / compiled batch exit 101**, restores production in finally, then
requires **8 passed / restored exit 0**. Final results: eight discriminating
controls, zero survivors. The restored gate covers **72 actual-source cases**.
Generated logs and JSON stay ignored under
`sdk-rs/target/phemex-rest-data-evidence/`; this manually authored sheet is tracked.

| Gate | Exit/result |
| --- | --- |
| Scoped Phemex all-targets cargo check, jobs 2 | 0 |
| Restored native rest_data test target, test threads 1 | 0; 8 tests / 72 cases |
| Extractor writer after manifest changes | 0; 1 normalized, 0 added/orphaned |
| Actual scoped Rust indexer/status check | 0; 79 rows / 466 public items / 0 errors |
| Tomllib count, transitions and declared target files | 0; 8 pending -> ported |
| Standalone merged Phemex source/tests preservation diff | 0 |
| Extended relevant source/configuration preservation diff | 0 |
| Cargo.lock package-record comparison | 0; only Weex changed |
| Post-merge extractor freshness | 0; 0 manifests would change |
| git diff --check | 0 |

The standalone `git diff --exit-code 3942045e69 HEAD --
sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests` returned 0
at the merge SHA above. Extended standalone comparisons preserve Phemex Cargo,
SDK Cargo configuration, Phemex manifests and TypeScript, shared TypeScript
helpers/types/math/log severity, Rust numeric/types/store/exchange/crypto/
transport/parity sources, extractor, package scripts and yarn lock. Tomllib
package-record comparison finds only safe-cex-venue-weex changed; every Phemex
dependency lock record is unchanged. Compiler/native/control/indexer exits
therefore stand on unchanged gated inputs; freshness ran after merge-down.

Coverage uses ordinary exchange DTOs and explicit UTF-16/binary64 edge cases,
UTC intraday/daily candle dates, fractional-millisecond TimeClip, and a throwing
from getter to verify explicit-limit short-circuiting. Original calendar/dayjs
semantics are delegated leaf primitives in production; the native adapter does
not claim exhaustive month/year, prototype/getter or sparse-array coverage.

Not run: full crate/workspace test suites, workspace-wide check, live exchange
calls, real credentials, browser automation, CI waiting, or PR creation/merge.
No shared type or trait was changed outside the Phemex crate.

Pre-push merge-down: `2560320bc9489046d130536411389d74fb718694`, integration
parent `d4b45c92c11acf2005262f792d26ac2a56a7cf05`. It adds BingX and Polymarket
work. Standalone Phemex source/test preservation against the implementation SHA
returns 0; the extended relevant-input diff also returns 0. Cargo.lock is
byte-identical to the first merge above (diff 0). Native/compiler/control/indexer
exits stand on unchanged gated inputs. Extractor freshness rerun on this merge
returns **0, 0 manifests would change**. Tomllib confirms **54 / 10 / 15**.


## Websocket data: eight pending rows moved

Base: `645f6fda62f3fab6787db45ab45688376008b739`.
Initial implementation: `e3d30d1fe6a0a97c347a1884b45d1cacd6294325`.
Final gated implementation: `9f7e7c3f59ffe7eb4d3abc3e8b4b4c97b0c91b9e`.
Merge-down: `03311fa9ed688ac26d6f5584fc9ce19005d9b3d1`, integration parent
`d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e`.
Branch: `feat/rust-phemex-websocket-data`.

Independent tomllib census: **60 ported / 12 divergent / 7 pending**, from
**54 / 10 / 15**. Transitions: **6 pending -> ported, 2 pending -> divergent**,
eight rows moved. All 72 off-pending rows have nonempty Rust targets in declared
existing files. The actual scoped Rust indexer/check uses default empty
exemptions and reports **79 rows, 520 public items, zero errors**.

The six ported rows cover packed ticker parsing, ticker reduction, candle
subscriptions, orderbook subscriptions, orderbook delta processing and private
message reduction. Rust owns their guards, requests, ordering, filters,
aggregation, state transitions and cleanup; opaque host projections retain
original external member calls, model references, coercion and shared helpers.
The native adapter witnesses real Pending snapshot reads. Each subscription has
its own parser token; book callbacks retain the same mutable book reference,
including after disposal clears it. The source candle timer can subscribe after
disposal, and that behavior is explicitly retained and witnessed.

The two class rows have individual CHOSEN notes. PublicSocket models all venue
fields/defaults, the packed handler and three binary64 static limits, using
owned fields and opaque dictionary/handler handles instead of prototype
inheritance and replaceable arrow-function properties. PrivateSocket models
request ID, undefined timeout, empty symbol storage and debounce using owned
fields, Option and a vector instead of the inherited runtime and JavaScript Set.
These rows claim state mappings; individual method bodies are mapped separately.

Host contract: clones share the original state and external member handles;
spawn retains continuations and immediately polls their synchronous prologue;
timers retain closures; read construction errors are distinct from asynchronous
rejections. Parser handles represent distinct closures and are registered with
the host dispatcher. These contracts preserve first-delta request/queue ordering
and cancellation guards without a venue-local JSON or transport conversion.

| Family | Actual-source cases | Production mutation | Named decoded-native assertion |
| --- | ---: | --- | --- |
| Public class | 1 | request ID 1200 -> 1201 | WS native complete observation public_class 0 |
| Private class | 1 | debounce 1000 -> 999 | WS native complete observation private_class 0 |
| Packed ticker | 7 | percentage multiplier 100 -> 101 | WS native complete observation packed 0 |
| Ticker stream | 9 | funding time 0 -> 1 | WS native individual effect ticker 1 |
| Private events | 31 | aop_p.subscribe -> aop_p.unsubscribe | WS native individual effect private 2 |
| Candles | 11 | row order [1,0] -> [0,1] | WS native individual effect candles 1 |
| Delta | 6 | amount multiplied by 2 | WS native individual effect delta 0 |
| Book | 17 | snapshot limit 1000 -> 999 | WS native individual effect book 1 |

The oracle executes actual TypeScript classes/bodies and actual shared helpers.
Native comparisons inspect UTF-16 strings, binary64 values, each effect and full
state. Outgoing request fields are asserted at the source JSON.stringify call
before decoded wire observations; no decode/re-encode equality is used as proof.
Independent case floors total **83**, across eight native tests. Cases include
lone surrogates, nonfinite numbers, strict candle interval checks, reversed rows,
duplicate subscriptions, queued stale/new deltas, disposal while pending,
synchronous read throws, rejected snapshots, retry/log throttling, terminal and
partial fills, closed-order overlays, zero-size position upserts, currency PnL
and cached null balances. Source lodash sums preserve null; only undefined falls
back to zero, corrected and covered in the final implementation.

`tests/websocket-data-controls.py` batches eight independent mutations in one
compiled negative program. Each family invokes its own production body; other
member/helper calls remain original host projections. Every named decoded-value
assertion above fails, **8 failed / batch exit 101**; finally restores source,
then requires **8 passed / restored exit 0**. Final controls: eight caught,
zero survivors. Generated logs/JSON remain ignored under
`sdk-rs/target/phemex-websocket-data-evidence/`. This sheet is manually authored,
tracked and append-only; earlier evidence bytes are preserved exactly.

| Gate | Exit/result |
| --- | --- |
| Final scoped Phemex all-targets check, jobs 2 | 0 |
| Final restored websocket_data target, test threads 1 | 0; 8 tests / 83 cases |
| Extractor writer after manifest changes | 0; 2 normalized, 0 added/orphaned |
| Final actual scoped Rust indexer/status check | 0; 79 rows / 520 public items / 0 errors |
| Tomllib count/transitions and declared target file checks | 0; 6 ported / 2 divergent transitions |
| Standalone own source/test preservation against final gated SHA | 0 |
| Extended relevant source/configuration preservation | 0, including Cargo.lock |
| git diff --check | 0 |

Standalone `git diff --exit-code 9f7e7c3f59 HEAD --
sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests` returns 0
at the merge SHA above. Extended standalone comparisons also return 0 for
Phemex Cargo/manifests/TypeScript, SDK Cargo/lock, shared TypeScript types/utils/
JSON parser/math/log severity, Rust numeric/types/store/exchange/crypto/transport/
parity, extractor, package scripts and yarn lock. The final compiler/native/
control/indexer exits stand on unchanged gated inputs after merge-down.

Coverage uses ordinary exchange DTOs and the explicit cases above; exhaustive
prototype/getter/sparse-array behavior and arbitrary class method replacement
are not claimed. No shared type or trait outside Phemex changed.
Not run: full crate/workspace test suites, workspace-wide check, host/wasm
clippy, live exchange calls, real credentials, browser automation, CI waiting,
or PR creation/merge.

Post-merge extractor freshness at `03311fa9ed688ac26d6f5584fc9ce19005d9b3d1`:
**exit 0, 0 manifests would change, 0 added/orphaned symbols**.

Pre-push merge-down: `facdfccfe30cb143d8ba347e022af51af6e6f257`, integration
parent `039d308cd99cfd09e22205fb1083c3df5a306548`. Standalone Phemex source/test
preservation against the final gated implementation returns 0; extended relevant
source/configuration preservation also returns 0, including Cargo.lock. The
compiler/native/control/indexer exits stand. Extractor freshness rerun on this
merge returns **0, 0 manifests would change, 0 added/orphaned symbols**.


## Startup and order formatting: remaining seven rows moved

Base: `89a8dc764a77c2ae0fa2e67a51aa28756f0cd895`.
Gated implementation: `b7157b54f4a7eb90324fd4a221b78452b8df62c5`.
Merge-down: `a2f62021d2430f62a642dbd57ef1e260eea7f8fa`, integration parent
`970c8a3a41e3ff7e0da31cbf524f7d6b0e064849`.
Branch: `feat/rust-phemex-start-order-formatting`.

Independent tomllib comparison measures **66 ported / 13 divergent / 0 pending**,
from **60 / 12 / 7**: **6 pending -> ported, 1 pending -> divergent**, seven
rows moved. All 79 off-pending rows have nonempty targets in declared existing
Rust files. The actual scoped Rust indexer/status checker with default empty
exemptions validates **79 rows, 539 public items, zero errors**.

The six ported rows are constructor, start, updateOrder, placeOrder,
formatCreateOrder and formatCreateTrailingStopLossOrder. ExchangeState is the
class row, with its own CHOSEN note: owned fields, Option slots and opaque
rule/API/socket handles replace prototype inheritance and replaceable arrow
function properties. The model covers every declared venue field/default and
all sixteen timeframes; constructor and method bodies are separately mapped.
Rule creation retains the original pattern helper/mint contract; native source
initializer probes compare label, requirement, validity probes and broker mint.

Constructor base/options/store references are retained. Source-visible receiver
assignments precede subsequent factory calls; native assertions verify original
options/store identity and partially initialized API/socket state. The secondary
API retains its original createAPI factory/rate-limit callback, maxRPS 3.
Startup retains both public completion tasks across an early rejection, updates
markets and starts the public socket at market completion, then joins/parses
and selects private/public-only startup using the previously captured flag.
Host clones share original state, spawn retains continuations, and asynchronous
entrypoints must be polled; original member/factory/property/helper calls remain
explicit contracts rather than venue-local JSON or transport conversions.

Order replacement retains source parent/child prefix matching, nullish updates,
parent overrides and cancellation-before-placement. Submission remains sequential,
with strict code checks, suspended-order handling, partial successes, last-error
selection, open-order updates before final error, and source duplicate emission.
Formatting preserves adjusted quantities/prices, trigger fields, reduce-only
rules, source ceil/remainder splitting (including source rounding behavior),
explicit undefined lot quantities and unconditional initial UUID plus fresh IDs
per final payload. Caller ID checks execute actual inherited BaseExchange helpers
in the oracle. The trailing formatter retains inverse position-side selection,
ordered guards, signed distance and original contracts; the existing disabled
trailing capability is unchanged.

| Family | Actual-source cases | Production mutation | Named decoded-native assertion |
| --- | ---: | --- | --- |
| Class | 1 | 1m resolution 60 -> 61 | ORDERS native complete observation class 0 |
| Constructor | 9 | maxRPS 3 -> 4 | ORDERS native individual effect constructor 0 |
| Start | 17 | tickers loaded flag redirected to orders | ORDERS native individual effect start 0 |
| Update | 12 | reduceOnly fallback false -> true | ORDERS native individual effect update 0 |
| Place | 14 | returned orderID read changed to wrongID | ORDERS native complete observation place 0 |
| Format | 22 | lot quantity multiplied by 2 | ORDERS native complete observation format 0 |
| Trailing | 9 | sell peg offset loses negative sign | ORDERS native complete observation trailing 0 |

Independent floors total **84 source cases / seven native tests**. The oracle
executes actual source bodies and actual shared adjust, inverseObj,
omitUndefined, capitalize, times and uuid helpers, plus inherited client-ID
methods and their helpers. Cases include constructor failures, concurrent and
reversed public completions, retained completion after rejection, disposal,
public-only flag changes, parent/SL/TP replacement, null/zero updates, sequential
mixed submission outcomes, missing models, quantities above max size, zero and
nonfinite values, scientific/string precision, caller ID rejection/reuse,
lone-surrogate IDs/symbols and trailing-stop failures/offsets. Tests inspect
native UTF-16/binary64 values, each effect and complete store state; futures
produce actual Pending. No wire roundtrip is used as a native-value proof.

`tests/start-order-controls.py` batches seven independent function mutations
into one compiled negative program. Constructor initialization uses the original
host initializer; other member calls remain original projections, so no family
calls another mutated body. Each named decoded-value assertion above fails:
**seven failed / compiled batch exit 101**. Finally restores production, then
requires **seven passed / restored exit 0**. Final controls: seven caught,
zero survivors. Generated logs/JSON remain ignored under
`sdk-rs/target/phemex-start-order-evidence/`; this manual sheet is tracked and
append-only, preserving earlier evidence bytes exactly.

| Gate | Exit/result |
| --- | --- |
| Scoped Phemex all-targets cargo check, jobs 2 | 0 |
| Restored start_order_formatting target, test threads 1 | 0; 7 tests / 84 cases |
| Extractor writer after manifest changes | 0; 1 normalized, 0 added/orphaned |
| Actual scoped Rust indexer/status check | 0; 79 rows / 539 public items / 0 errors |
| Tomllib transitions and declared target file checks | 0; 6 ported / 1 divergent transitions |
| Standalone own source/test preservation against gated implementation | 0 |
| Focused Source/math/Phemex configuration preservation | 0 |
| Cargo/package configuration semantic comparisons | 0; unrelated changes only |
| Post-merge scoped all-targets check | 0 |
| Post-merge native target | 0; 7 tests / 84 cases |
| Post-merge extractor freshness | 0; 0 manifests would change |
| git diff --check | 0 |

Standalone `git diff --exit-code b7157b54f4 HEAD --
sdk-rs/safe-cex-venue-phemex/src sdk-rs/safe-cex-venue-phemex/tests` returns 0
at the merge SHA above. The extended comparison was **1**, not a preservation
pass: shared exchange/transport code changed, the workspace gained the Lighter
SDK member/lock record, and package.json extended only the Kucoin oracle script.
Affected scoped compiler/native gates were rerun and passed. Python comparisons
prove no other workspace configuration, Phemex dependency lock records or
package scripts changed. A focused byte diff returns 0 for Phemex Cargo/
manifests/TypeScript, actual TS base/client-ID helpers, shared TS types/utils/
math/log severity, Rust numeric/types/store/crypto/parity, extractor and yarn lock.
Seven mutation controls stand on unchanged venue bodies and unchanged actual
source/math/host inputs; no new control matrix was run for unrelated root methods.

Coverage uses ordinary DTOs and the explicit cases above; exhaustive prototype,
getter and sparse-array behavior is not claimed. No shared type/trait outside
Phemex was changed by this slice. Not run: full crate/workspace suites,
workspace-wide check, host/wasm clippy, live exchange calls, real credentials,
browser automation, CI waiting, or PR creation/merge.

Pre-push merge-down: `baf1d253b41896b0021b2199bfefc1099c3bdcbb`, integration
parent `98a847bb171d9a895a95f1e4e287c9126fc2c66e`. Own source/tests remain
byte-identical to the gated implementation (standalone diff 0). Relevant source
and configuration inputs are unchanged from the first merge (diff 0 excluding
Cargo.lock). Tomllib package-record comparison finds only Lighter SDK gained
safe-cex-num; every Phemex dependency record is unchanged. Post-merge compiler/
native/indexer exits and seven control exits stand. Extractor freshness rerun
on this merge is **0, 0 manifests would change, 0 added/orphaned symbols**.
Final measured Phemex triple: **66 / 13 / 0**, all 79 rows targeted in Rust.

Last pre-push merge-down: `4fefda9249334a507e931301dcbc8033fc98d175`, integration
parent `4afb5dbf48e5f071608ee23c74e367a107695439`. It changes Bybit/Woo work;
Phemex source/tests and every relevant extended input, including Cargo.lock,
are byte-unchanged from the previously recorded merge (standalone diffs 0).
Compiler/native/control/indexer exits stand. Extractor freshness rerun on this
merge returns **0, 0 manifests would change, 0 added/orphaned symbols**.
Tomllib again measures **66 / 13 / 0** with all 79 rows off pending.

## Production Exchange binding and owned finding repairs (2026-10-16)

`binding/exchange.rs::PhemexExchange::new` assembles the existing venue bodies
with the shared Js runtime, base exchange, Store and socket facades. The typed
Exchange entry points execute signed HTTP, order formatting/cancellation,
settings changes and public/private startup. Tests use distinct amount/price
steps, caller IDs, suspension across disposal, public-only mode, delayed remote
signing, failed signing and auth acknowledgement before subscription.

The 23 `tests/exchange_binding.rs` E2E tests passed through production construction.
The four `tests/binding_values.rs` tests passed, including actual TypeScript
observations for every one of the 2048 lone-surrogate units. Raw ticker output
retains the response object and D800USDT symbol; formatSymbolOut retains raw
info.symbol D800. The nine private transport tests passed, including the actual
source oracle and a new account-read/timer/queue observation before future polling.

`scripts/binding-controls.py` passed all 23 compiling production controls:
19 Exchange method/disposal controls and four helper controls. Each mutation
failed its named runtime test with exit101, then the exact restored bytes passed
with exit0. The four repaired findings are formatSymbolIn, formatSymbolOut,
fetchTickersRaw and eager private reconciliation. Moving begin_sync back behind
the native future boundary is the eager-start negative control.

Five typed-edge FINDING rows remain, as assigned by the overseer to shared
string-model task ddi2il-10: PhemexApiResponse.msg, POSITIONS_RESPONSE.account.currency,
getAccount.userId, validateAccount.error, and parseTickers' symbol/Store key.
They are Rust representation losses, not TS-carried defects, and remain marked
as findings. The source's missing ticker-error emit(undefined)/null-return behavior
is carried by the production reader; no fixture silently fixes it.

Readiness checkpoint before the Crypto.com registry follow-up: full scoped
all-targets suite passed 91 tests across 17 targets, zero ignored. Strict host
all-targets and wasm library Clippy passed. Canonical workspace parity passed
(7459 rows / 17060 public Rust items) after adding exact binding split targets
and detached AccountRead mapping; those mapping edits did not change Rust code.
Registry integration remains to be added in the Phemex PR.

## Registry and source-super disposal integration

Phemex is now in the Rust registry, with native Js HTTP, hosted signing, the
external-data factory and the shared limiter host API. The native HTTP witness
checks distinct0.003 amount/0.25 price steps, read purity, user123 and HMAC over
the actual currency=USDT account query. Its compiling wrong-factory mutation
fails101 and the exact restored table passes0.

The source Phemex dispose calls super.dispose, unlike Crypto.com. The wrapper
now delegates once through the bound source body, with no extra early base call.
The new E2E observes the disposed flag and removed inherited listener; skipping
SessionHost::base_dispose compiles and fails that regression, then restores green.
The original source-dispose-call suppression control also fails/restores.

Full scoped suite:92 tests across17 targets, zero ignored;24 binding E2E tests.
After master185/164 integration,37 binding/helper/private-transport tests and14
native construction/table tests passed. The shared production dispatch guard
passed after reviewing21 new Phemex call/reference records; its sole base cleanup
call is the source-super adapter with explicit source/native evidence.
The original23 method/helper control checkpoint remains valid; the added skip-super
control makes24 and the registry factory control is separate.

Final integrated static gates passed for Phemex plus registry: strict host
all-target and wasm library Clippy; canonical parity7459 rows /17114 public
items; layers88/88; live source registry48 ordered names. Read-only extractor
freshness reports0 changed manifests,0 added and0 orphaned symbols after taking
the author's formatting-only #190. Five shared typed-edge findings stay marked
under ddi2il-10; these gates do not reclassify them as ported.

## Native tunnel transport provider (2026-10-17)

The production native host supplies Phemex's requested installer through shared
`js_runtime::client::factory::install_transport_adapter` using the same HTTP host,
runtime transport provider, options/name, and source default adapter variant.
`safe-cex-registry/tests/native_host_tunnel.rs` reads markets and a signed account
query over a real local tradetunnel. It verifies rpc.auth token, two rpc.http
requests, independent SHA256 HMAC over observed query/expiry bytes, distinct
amount/price steps, Store read purity and zero direct-origin HTTP.

Before the fix, the new witness failed101 at construction with the exact missing
installTransportAdapter-provider error. The restored implementation passes that
witness and the existing Binance tunnel test. Permanent compiling absent/no-op
provider controls live in registry/scripts/phemex-tunnel-controls.py.

## Direct flattened-envelope numeric decoding (2026-10-17)

The complete-record witness initially passed Account, Position and PositionsResponse
but failed PhemexApiResponse<PositionsResponse> at direct text decoding with
`invalid type: map, expected f64`. Serde flatten had buffered lexical numeric
payload fields through Content. The replacement streams metadata and payload
fields from the original map without adding payload trait bounds or a dependency.

Seven native witnesses now pass: all 49 declared account/position fields and their
numeric bits, complete valid Serde-order arrays, text/owned/borrowed/qualified and
numeric-ingress paths, borrowed payloads, metadata presence/duplicates/order and
custom payload early completion. The existing live-source type corpus also passes
its 450 native comparisons. Host all-target and wasm library Clippy pass. The
permanent envelope-controls.py restores the former derive: it compiles and fails
the named numeric witness with exit101, then exact restoration passes. Canonical
parity passes (7459 rows,17183 public Rust items). These observations do not clear
the existing surrogate FINDING rows or classify private helper decoders.

## Original accepted-contract census

The original native/default census selects exactly 44 owners. The reviewed
per-crate shard produces 30 native decoder results and 14 individual
classifications, with zero failed selected owners. Four record observations
cover all 49 account/position fields, numeric bits, metadata and cardinality via
six decoder paths; 26 function owners execute 136 independently authored cases.
Object observations use recursive native Js fields/kinds/number bits rather
than serialized numeric lexemes. The shared-Js host fixture panics on unrelated
capabilities and its mapped-key corpus is explicitly limited to exact keys.

Twelve non-decoder helpers have direct native unit witnesses and twelve
compiling named-failure/exact-restoration controls, including three new outbound
controls in scripts/outbound-contract-controls.py. Three guard-only record
controls compile after widening, fail the named complete-array rejection
assertion, and pass after exact restoration. The direct-envelope old-derive numeric control remains in #280.
Two typed initializer classifications have separately executed source/native
constructor witnesses. The Api construction record reviews only shared-handle
Clone capability and makes no executed clone or method-decoding claim.

Reproduce selected compiler/native checks with
`node safe-cex-venue-phemex/scripts/accepted-census-contracts.mjs` from sdk-rs;
see safe-cex-parity/accepted-set/PHEMEX-REVIEW.md for individual proof commands.
This does not assert full current discovery: ten post-census owners, one public
alias, local scopes, feature combinations and WASM remain separate obligations.


## Post-merge #418: real snapshot failure warnings

At master b07c56cb67, test-only62ccac890c/c075109733 constructs the actual Phemex
Exchange public book and rejects terminal HTTP. The real shared client, socket,
snapshot continuation and warning path run unchanged. The maintained TS class
and ENDPOINTS declaration run independently in snapshot-warning-source.cjs.

Compiled RED1: the parent-dispatch bug produces zero warnings instead of one
(`/tmp/port-txflow-phemex-warning-parent-red.log`). WsHost routes log to the
socket's nonexistent member instead of parent.log. With only that dispatch
repaired, compiled RED2 fails the named PHEMEX-SNAPSHOT-WARNING assertion:
`Error: unscripted request ...` instead of the original Error.message
(`/tmp/port-txflow-phemex-warning-message-red.log`). Both compilation failures
are excluded: these are executed test failures, each exit101.

The complete fix dispatches log to parent and selects Error.message for native
Error/AxiosError instances, retaining String(value) for other thrown values.
The real regression passes next-delta retries, one warning inside the30-second
window, reopening after30seconds, exact severity/text, no escaped runtime error,
and recovery to a valid snapshot merged with its initial delta. The actual read
limiter remains enabled; the fixture advances its clock between retry attempts.
The source supplement separately asserts Error, ordinary-object and string
rejections. All36 constructed binding tests pass, including the original three
#418 chart history/candle/book regressions. No original oracle corpus changed.

Reproduce from root:

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-phemex --jobs 1 --test exchange_binding
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" node sdk-rs/safe-cex-venue-phemex/tests/snapshot-warning-source.cjs
```

Review scope also traced source symbol formatting, parent timeframe/xhr selection,
REST field mapping and the snapshot await/delta boundary from #418. The daily UTC
normalization gap is already owned by #825; this PR makes no date changes.

Final gates: native and wasm all-target Clippy -D warnings and fmt pass. The
unchanged REST and WS actual-source suites each pass8 (16 total). Scoped parity
passes7459 rows/547 public items with zero Phemex exemptions, and extractor
changes zero manifests. All-features cancellation inventory6PASS/1exportignore;
override contracts7PASS/1live-sourceignore. Both changed bodies are synchronous
and neither has a cancellation/override body pin; the inventories pass unchanged.
Logs: `/tmp/port-txflow-phemex-warning-{green,oracles,source,clippy-native,clippy-wasm,parity,extract,inventory}.log`
(source output is `.json`). No shared body/transport API changed.
