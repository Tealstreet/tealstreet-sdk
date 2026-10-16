# Telegram worker gate evidence

**Moved 3 pending → ported, 4 → divergent.** Initial parent `2702cf272626f1a64a60f07ba9a750f722185c2b`. Implementation/control SHA `5c25bf49caf56dfc786cd7a42915b7574481566e`. Reviewed integration `0f02e1219579430184111306d15e12e7f0864141` merged down to `9330e43fd3636061f1a9fd94e6194edd70915af9`; fresh gates below measured there. The delivery documentation commit changes only this tracked evidence sheet. No unreviewed external feature work is an ancestor.

## Scope and representation

Ports all seven manifest rows in TelgramWorker.ts. Constructor, stored callback, and exchange accessor are ported: generic opaque token/result carriers, exact Rc callable identity, an explicit receiver for JavaScript this, one fresh invocation per access, +0 counter and empty cache initialization. Runtime class/start/process/tick rows are specifically CHOSEN or MIXED, with mandatory shared Position UTF-8 restrictions and chosen typed Telegram records, host Axios-equivalent operations, canonical errors, native ownership/polling and exception reporting. The manifest contains each row’s reason; there is no blanket parity claim for JS reflection, malformed/inherited payloads, microtask ordering, or arbitrary thrown values.

Native GET/POST construction occurs before future polling. The host executor interface represents synchronous construction errors with Result and asynchronous rejection with a returned future; its default wrapper uses existing command registration and omitted request ID. Host functions must return errors through that contract; Rust panics are not caught as JS exceptions. The native host test adapter supplies HTTP behavior; no automatic HTTP adapter or live Telegram/venue run is claimed.

## Native evidence floors

The actual TypeScript class is transpiled and executed with fixture HTTP, executor and timer imports. Independent expectations stop at: four constructor cases (last lone-surrogate UTF-16 token), eighteen message cases (last repeated direct /positions), fourteen polling checkpoints (last pending GET settlement), three start/timer checkpoints (last first timer firing). Five Rust tests pass, including a separately scoped native-only default command-slot adapter test. No case count is derived from the output being tested.

Native assertions precede each source-profile comparison: opaque token units, zero counter/cache and callback identity; receiver pointer/token reads and returned object identity with live replacement; decoded native chat ID bits; exact native POST text/URL/chat value, executor arguments, event ordering, immediate sync/getter catch prefixes, absent-executor no-op, direct duplicate/repeated dispatch without cache writes; first nine cache-only responses, tenth dispatch-before-caching, ignored-ID caching, cross-sign zero and distinct signaling/negative NaN keys, failed/pending GET state, 1000ms scheduling and GET prefix before start polling. The source preserves a lone-surrogate CLI symbol while typed native message JSON refuses it. Opaque constructors preserve lone UTF-16 token units; runtime UTF-8 bounds are separately divergent. Extra NaN bit queries and the default command-slot adapter are native contract witnesses, not an expanded source enumeration. Profiles are field observations, not native decode/re-encode roundtrips.

## Measured exits

All oracle cargo commands use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`; cargo commands ran sequentially with jobs 2.

| SHA | Gate/control | Exit |
| --- | --- | --- |
| `5c25bf49ca` | `telegram-clean` | 0 |
| `5c25bf49ca` | `telegram-constructor-counter-mutant` | 101 |
| `5c25bf49ca` | `telegram-constructor-counter-restored` | 0 |
| `5c25bf49ca` | `telegram-getter-double-call-mutant` | 101 |
| `5c25bf49ca` | `telegram-getter-double-call-restored` | 0 |
| `5c25bf49ca` | `telegram-callback-identity-mutant` | 101 |
| `5c25bf49ca` | `telegram-callback-identity-restored` | 0 |
| `5c25bf49ca` | `telegram-nan-key-mutant` | 101 |
| `5c25bf49ca` | `telegram-nan-key-restored` | 0 |
| `5c25bf49ca` | `telegram-warm-threshold-mutant` | 101 |
| `5c25bf49ca` | `telegram-warm-threshold-restored` | 0 |
| `5c25bf49ca` | `telegram-scoped-check` | 0 |
| `5c25bf49ca` | `telegram-callback-valid-mutant` | 101 |
| `5c25bf49ca` | `telegram-callback-valid-restored` | 0 |
| `9330e43fd3` | `telegram-merge-check` | 0 |
| `9330e43fd3` | `telegram-merge-tests` | 0 |
| `5c25bf49ca` | extractor check | 0 |
| `9330e43fd3` | fresh extractor check | 0 |
| `14d714a73b` | delivery scoped check | 0 |
| `14d714a73b` | delivery extractor check | 0 |

Exact clean commands:

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test telegram --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Manifest metadata was serialized with parseManifest/serializeManifest, then the writer parity:extract ran before the implementation commit (exit 0, 0 updated/added/orphaned). Both measured extractor checks reported 7341 symbols across 512 sources and zero updates/additions/orphans; output text was inspected as well as measured exits. Earlier uncommitted fixture preflights needed compiler fixes; they are not reported as commit-bound gates.

## Discrimination

Each mutation is an uncommitted change relative to the implementation SHA, restored exactly before its clean rerun. No survivor was observed among the five valid controls; no broader mutation census is claimed.

| Mutation/call site | Named native failure | Mutant/restored exits |
| --- | --- | --- |
| new initializes tick counter to 1 | TELEGRAM native initial counter | 101 / 0 |
| exchange invokes callback twice | TELEGRAM native getter first call count | 101 / 0 |
| new wraps callable in a distinct Rc (temporary static bounds allow the wrapper) | TELEGRAM native callback identity | 101 / 0 |
| number_key omits NaN normalization | TELEGRAM native signaling NaN seen | 101 / 0 |
| tick changes warm-up threshold from 10 to 9 | TELEGRAM native tick effects warm-9 | 101 / 0 |

The first callback wrapper attempt failed compilation with E0310 (Token lifetime), exit 101, without executing any assertion; its restored run exited 0. That attempt is **inconclusive**, not discrimination proof or a runtime survivor. The corrected control supplies both necessary static bounds, compiles, and fails the named callback-identity assertion. Its restoration passes. Both measured attempts remain in the exit table.

## Merge, census and review

`git diff --exit-code 5c25bf49ca HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0** at `9330e43fd3`. Dependency sources and extractor inputs changed in integration; therefore scoped check, all five Telegram tests and extractor were refreshed at this merge SHA. Mutation source/test inputs stayed byte-unchanged and the controls were not repeated. No CLAUDE conflict occurred.

Python3.12 tomllib parsed all 38 tracked worker manifests, stopping at file38 / 222 rows, matching identities against both explicit initial and merged integration parents: 104 ported / 59 divergent / 59 pending → **107 / 63 / 52**. The seven identity changes are exactly this slice’s manifest; no imported census movement is claimed.

Sub-agent context_review performed read-only design/code/fixture/mapping reviews and ran no gates. It found missing receiver ABI, insufficient pre-drain catch ordering, narrow NaN payload coverage, and an untested default executor wrapper. The code/tests address each: explicit receiver, immediate prefix assertions, cross-payload queries, Result construction adapter, and native default-slot test. Final review found no blockers.

## Not run

No workspace-wide build, full monorepo suite, host clippy, wasm clippy/build, browser/worker deployment, live Telegram or venue tests, or CI rerun. Scheduling tests are controlled host floors and do not prove JavaScript microtask identity.

### telegram-merge-check output

```text
    Checking safe-cex-types v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-types)
    Checking safe-cex-num v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-num)
    Checking safe-cex-crypto v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-crypto)
    Checking safe-cex-transport v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-transport)
    Checking safe-cex-candle-cache v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-candle-cache)
    Checking safe-cex-store v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-store)
    Checking safe-cex-exchange v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-exchange)
    Checking safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 44.09s
```

### telegram-merge-tests output

```text
   Compiling safe-cex-types v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-types)
   Compiling safe-cex-num v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-num)
   Compiling safe-cex-crypto v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-crypto)
   Compiling safe-cex-candle-cache v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-candle-cache)
   Compiling safe-cex-transport v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-transport)
   Compiling safe-cex-store v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-store)
   Compiling safe-cex-exchange v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-exchange)
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 22.23s
     Running tests/telegram.rs (sdk-rs/target/debug/deps/telegram-0918909bd8d975f2)

running 5 tests
test default_host_reads_registered_executor_and_omits_request_id ... ok
test opaque_constructor_callback_and_live_getter_match_source ... ok
test start_issues_get_before_poll_and_timer_restarts_polling ... ok
test message_dispatch_preserves_commands_duplicates_and_error_prefixes ... ok
test nine_warm_ticks_cache_ignored_ids_and_preserve_request_settlement ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```

### telegram-merge-extract-check output

```text
yarn run v1.22.21
$ node -e "const p='sdk-rs/parity/extract-ts/src/cli.ts';try{if(!require('node:fs').statSync(p).isFile())throw Error()}catch{console.error('Parity extractor has not landed yet: '+p);process.exit(1)}" && node --import tsx sdk-rs/parity/extract-ts/src/cli.ts --check
Symbols: 7341 across 512 source files
By kind:
  function: 313
  method: 633
  field-fn: 3006
  constructor: 137
  accessor: 115
  class: 212
  const-fn: 1214
  const: 592
  variable-fn: 0
  variable: 3
  type: 1081
  enum: 35
By top-level directory:
  (root): 146
  accounts: 20
  candleCache: 43
  config: 11
  crossTrade: 31
  exchanges: 5866
  health: 27
  http-over-websocket: 102
  httpOverWebsocket: 2
  instrument-class: 20
  journal: 33
  json-parse: 2
  lighter-sdk: 389
  poseidon-crypto: 22
  safe-cex-react-native: 3
  safe-cex-server: 3
  safe-cex-web: 11
  script-api-core: 27
  store: 83
  transport: 63
  utils: 215
  webworker: 222
Would update 0 manifests; 0 added, 0 orphaned symbols.
Done in 13.16s.
```

### telegram-callback-valid-mutant output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 20.37s
     Running tests/telegram.rs (sdk-rs/target/debug/deps/telegram-fe8b39c3c56b2352)

running 5 tests
test default_host_reads_registered_executor_and_omits_request_id ... ok
test opaque_constructor_callback_and_live_getter_match_source ... FAILED
test start_issues_get_before_poll_and_timer_restarts_polling ... ok
test nine_warm_ticks_cache_ignored_ids_and_preserve_request_settlement ... ok
test message_dispatch_preserves_commands_duplicates_and_error_prefixes ... ok

failures:

---- opaque_constructor_callback_and_live_getter_match_source stdout ----

thread 'opaque_constructor_callback_and_live_getter_match_source' (15353142) panicked at safe-cex-webworker/tests/telegram.rs:390:9:
TELEGRAM native callback identity
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    opaque_constructor_callback_and_live_getter_match_source

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s

error: test failed, to rerun pass `-p safe-cex-webworker --test telegram`
```

## Final merge before push

Fetched and merged reviewed integration `9c151409d125381f67d08d2b644afa89c77a79c8` to `14d714a73b7762d0e6b838526835d8c5600da9da`. `git diff --exit-code 9330e43fd3 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0**. The broader diff across all worker dependency source directories and its Cargo.toml/lock also measured exit 0. Thus the five runtime tests at 9330e43fd3 remain valid; the last merge changes unrelated venue work and parity metadata/exemptions. No CLAUDE conflict occurred.

Scoped all-target check and extractor were both measured fresh at 14d714a73b: **exit 0**. Extractor text again reports 7341 symbols, zero updates/additions/orphans. Python3.12 tomllib compared to final integration 9c151409d1 and confirmed 104/59/59 →107/63/52, three pending→ported and four→divergent. The final evidence commit changes this sheet only.

### telegram-delivery-check output

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.60s
```

### telegram-delivery-extract-check output

```text
yarn run v1.22.21
$ node -e "const p='sdk-rs/parity/extract-ts/src/cli.ts';try{if(!require('node:fs').statSync(p).isFile())throw Error()}catch{console.error('Parity extractor has not landed yet: '+p);process.exit(1)}" && node --import tsx sdk-rs/parity/extract-ts/src/cli.ts --check
Symbols: 7341 across 512 source files
By kind:
  function: 313
  method: 633
  field-fn: 3006
  constructor: 137
  accessor: 115
  class: 212
  const-fn: 1214
  const: 592
  variable-fn: 0
  variable: 3
  type: 1081
  enum: 35
By top-level directory:
  (root): 146
  accounts: 20
  candleCache: 43
  config: 11
  crossTrade: 31
  exchanges: 5866
  health: 27
  http-over-websocket: 102
  httpOverWebsocket: 2
  instrument-class: 20
  journal: 33
  json-parse: 2
  lighter-sdk: 389
  poseidon-crypto: 22
  safe-cex-react-native: 3
  safe-cex-server: 3
  safe-cex-web: 11
  script-api-core: 27
  store: 83
  transport: 63
  utils: 215
  webworker: 222
Would update 0 manifests; 0 added, 0 orphaned symbols.
Done in 15.82s.
```
