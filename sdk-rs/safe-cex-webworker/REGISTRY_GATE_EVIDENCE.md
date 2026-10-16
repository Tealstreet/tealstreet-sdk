# Postprocessor registry gate evidence

**Moved 1 pending → ported, 2 → divergent.** Initial parent: `ed8336d5835189fd470040fbe62756ff1b60a0ae`. Registry gate/control SHA: `9ff5c66d57841d46e56b1b4ef14bed4cb566c960`. Adapter repair gates: `45312421457eacc36a934434dcb9d509fab1ae63`. Fresh post-merge gates: `697440bed82e8d3700d443a4dbea5cd463b9236d` after merging reviewed integration `e150dc59a2d5db2af0ff1875c92f291f2f32b9f4`. The following delivery documentation commit changes only this tracked evidence file. No unreviewed external feature work is an ancestor.

## Scope and mappings

`registerIndependentPostprocessor` ports constructor-object identity deduplication and appends the original handle to the same shared independent list. The two list exports are **CHOSEN divergent**: explicit host-injected constructor linking and caller-owned registry lifetime replace automatically initialized module globals. Hosts must retain one registry and canonical Rc constructor handles; wrapping the same factory twice creates distinct objects, just as distinct source constructor objects remain distinct. The generic constructor payload is not coerced or instantiated by registration. No automatic binding of the seven production processors is claimed.

The oracle transpiles and executes the actual source registry with seven imported constructor fixtures. Rust uses callable factory fixtures at that same dependency boundary. The nine-step floor ends at index 8, dependent-list reordering: initial arrays, builtin duplicate, append, repeated extension, same-name distinct object, external removal, builtin reappend, independent reversal plus duplicate, dependent reversal. Independent fixed identity-index expectations determine cardinality and order; dependent expectations are literal three-item arrays. Source and constructed-profile cardinalities are checked against those expectations. Native Rc identity, retained list identity, constructor result name/value bits, and dependent order/results are asserted before source-profile comparisons. No decode/re-encode roundtrip is used.

## Measured gates

All oracle cargo tests use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. All cargo commands are sequential, `--jobs 2`.

| Measured SHA | Gate | Exit |
| --- | --- | --- |
| `9ff5c66d57` | `registry-runtime-clean` | 0 |
| `9ff5c66d57` | `registry-dedupe-mutant` | 101 |
| `9ff5c66d57` | `registry-dedupe-restored` | 0 |
| `9ff5c66d57` | `registry-scoped-check` | 101 |
| `de22163fcb` | `registry-repair-check` | 101 |
| `78ba9c5ab8` | `registry-repair-check` | 101 |
| `4531242145` | `registry-repair-check` | 0 |
| `4531242145` | `registry-repair-runtime` | 0 |
| `4531242145` | `registry-repair-timers` | 0 |
| `8e576d236d` | `registry-merge-check` | 0 |
| `8e576d236d` | `registry-merge-runtime` | 101 |
| `2bd1d02802` | `registry-merge-check` | 0 |
| `2bd1d02802` | `registry-merge-runtime` | 0 |
| `2bd1d02802` | `registry-merge-timers` | 0 |
| `697440bed8` | `registry-final-check` | 0 |
| `697440bed8` | `registry-final-runtime` | 0 |
| `697440bed8` | `perpetual-native-mutant` | 101 |
| `697440bed8` | `perpetual-native-restored` | 0 |
| `9ff5c66d57` | extractor check | 0 |
| `8e576d236d` | post-merge extractor check | 0 |
| `2b164e4108` | delivery scoped check | 0 |
| `2b164e4108` | delivery extractor check | 0 |

Exact commands:

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test postprocessor_registry --locked --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test postprocessor_registry --test dev --test message_stats --test twap_state --test twap_postprocessor --test store_preparation --test task_identity --locked --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --lib subscriptions::tests --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

The manifest was changed with parseManifest/serializeManifest and followed by the writer `parity:extract` (pre-commit exit 0, text: 0 updated, 0 added, 0 orphaned). The check outputs were inspected, not judged by the yarn shim status alone.

## Discrimination and compile floor repairs

At `9ff5c66d57841d46e56b1b4ef14bed4cb566c960`, an uncommitted mutation replaced the registration identity guard with unconditional append. The named native assertion **REGISTRY native cardinality step 1** failed: 5 retained constructors instead of the independent expectation 4; exit **101**. Restoring the original source gave exit **0**. Registry source, test, and oracle were byte-unchanged across later adapter repairs (scoped diff exit 0). This single control was caught; no survivor or exhaustive mutation inventory is claimed.

The first scoped check (9ff5c66d57) failed E0046 because tests/support/runtime.rs lacked newly required ExchangeRuntime::monotonic_now_ms. After adding the deterministic sleep clock, the de22163fcb check failed E0407 because tests/support/ws_runtime.rs still implemented removed WsRuntime::monotonic_ms. After deleting that retired override, the 78ba9c5ab8 check exposed E0046 on tests/twap_state.rs::DrawRuntime. These are three measured failed checks, not passing compile claims.

The final adapter repairs add the same virtual clock used for sleep, inherit it through WsRuntime, and make an unexpected elapsed-clock read panic in DrawRuntime (state creation must use the wall clock, like its other forbidden hooks). Scoped compilation now passes. The selected runtime targets passed five tests (dev 1, message_stats 1, registry 1, twap_state 2), and the filtered subscriptions library target passed two tests. These existing tests validate the changed adapters without a repository-wide suite.

## Merge proof, census, and review

At merge SHA 8e576d236d, the scoped source/tests comparison to 4531242145 measured exit 0, but dependency inputs changed. The fresh scoped check passed; selected runtime tests failed TWAP-MARKET identity cases execute (26 actual, independently expected 24). Integration added isPerpetual, one Boolean leaf with two false/true cases. Commit 2bd1d02802 adds the direct native field capture and exactly two native false/true assertions, and updates the fixed market count to 26. Commit 697440bed8 updates the remaining consumers: TWAP projection and snapshot market counts 24→26; task options cases/leaves 47/33→49/34, task state 44/31→46/32. The +2 cases / +1 leaf expectation comes from the added Boolean specification, never the measured profile length. Source/tests differ from the earlier gate after these test repairs; the final scoped check and all seven affected targets were refreshed at 697440bed8 (nine tests passed). The two subscription tests passed at 2bd1d02802 and their inputs are unchanged by the final test-only cardinality edits. No CLAUDE conflict occurred. The registry source/control inputs remained byte-unchanged, so its mutation was not repeated.

Python 3.12 tomllib parsed 38 tracked webworker manifests, stopping at file 38 / 222 rows. Initial and merged integration census: 103 ported / 57 divergent / 62 pending → **104 / 59 / 59**. The three changed identities are independentPostprocessors, registerIndependentPostprocessor, exchangeDependentPostprocessors; only registration is newly ported. No census movement outside this slice is claimed.

Sub-agent context_review reviewed the registry design, implementation, coverage fixes, runtime adapter changes, and the perpetual-field counts/assertions; no blockers remained. The agent ran no gates. Review identified missing source constructed-profile cardinality and an unmatched dependent-array mutation; both were fixed before the committed gate/control SHA.

## Limits and gates not run

The constructors in this test are dependency fixtures, not the seven production implementations. Hosts supply/import those implementations separately. The native RefCell interface requires callers to release outstanding list borrows before registry mutation; conflicting borrows can panic. The deterministic test runtime shares its wall and elapsed clock and does not independently exercise wall-clock adjustments. These tests are a nine-step floor, not exhaustive dynamic-array or host/runtime proof.

No workspace-wide build, full monorepo suite, host clippy, wasm clippy/build, browser/worker deployment, live venue test, or CI rerun was performed.


The new perpetual-field native assertion was also discriminated at 697440bed8: an uncommitted create_new_state mutation forced output.active_market.is_perpetual to Some(false). TWAP-MARKET native perpetual boolean failed on the independently expected true witness (exit101). Restored source passed (exit0). No survivors in the two controls run for this slice. The extractor inputs are unchanged since its passing 8e576d236d run.
### perpetual-native-mutant output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 22.39s
     Running tests/twap_state.rs (sdk-rs/target/debug/deps/twap_state-2e06290632c055fe)

running 2 tests
test source_market_mutation_cannot_rewrite_the_captured_input ... ok
test new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses ... FAILED

failures:

---- new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses stdout ----

thread 'new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses' (14379022) panicked at safe-cex-webworker/tests/twap_state.rs:153:13:
assertion `left == right` failed: TWAP-MARKET native perpetual boolean
  left: Some(false)
 right: Some(true)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s

error: test failed, to rerun pass `-p safe-cex-webworker --test twap_state`
```

### perpetual-native-restored output

```text
    Blocking waiting for file lock on package cache
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.95s
     Running tests/twap_state.rs (sdk-rs/target/debug/deps/twap_state-2e06290632c055fe)

running 2 tests
test source_market_mutation_cannot_rewrite_the_captured_input ... ok
test new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.42s
```

### registry-final-check output

```text
    Checking safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 36.84s
```

### registry-final-runtime output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 7.31s
     Running tests/dev.rs (sdk-rs/target/debug/deps/dev-f9a1d5ca7a805125)

running 1 test
test dev_dispatch_reaches_real_socket_and_regional_how_targets ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s

     Running tests/message_stats.rs (sdk-rs/target/debug/deps/message_stats-2e26d152f90a181d)

running 1 test
test message_stats_match_live_socket_precedence_and_pin_shared_representation_gaps ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s

     Running tests/postprocessor_registry.rs (sdk-rs/target/debug/deps/postprocessor_registry-be4f67ea7bd49b3c)

running 1 test
test constructor_lists_keep_order_identity_and_live_mutation ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s

     Running tests/store_preparation.rs (sdk-rs/target/debug/deps/store_preparation-5f0240bb638f6e25)

running 1 test
test store_payload_preparation_matches_live_full_snapshots_and_incremental_transitions ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s

     Running tests/task_identity.rs (sdk-rs/target/debug/deps/task_identity-776a47ee1fde4e29)

running 1 test
test task_native_field_identity_survives_outer_decode ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s

     Running tests/twap_postprocessor.rs (sdk-rs/target/debug/deps/twap_postprocessor-1c24fdbfce85d3bb)

running 2 tests
test source_market_mutation_cannot_rewrite_the_captured_input ... ok
test projection_matches_live_manager_filter_spread_order_and_aliases ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.17s

     Running tests/twap_state.rs (sdk-rs/target/debug/deps/twap_state-2e06290632c055fe)

running 2 tests
test source_market_mutation_cannot_rewrite_the_captured_input ... ok
test new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.33s
```

### registry-dedupe-mutant output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.94s
     Running tests/postprocessor_registry.rs (sdk-rs/target/debug/deps/postprocessor_registry-be4f67ea7bd49b3c)

running 1 test
test constructor_lists_keep_order_identity_and_live_mutation ... FAILED

failures:

---- constructor_lists_keep_order_identity_and_live_mutation stdout ----

thread 'constructor_lists_keep_order_identity_and_live_mutation' (14008990) panicked at safe-cex-webworker/tests/postprocessor_registry.rs:114:9:
assertion `left == right` failed: REGISTRY native cardinality step 1
  left: 5
 right: 4
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    constructor_lists_keep_order_identity_and_live_mutation

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s

error: test failed, to rerun pass `-p safe-cex-webworker --test postprocessor_registry`
```

### registry-dedupe-restored output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.53s
     Running tests/postprocessor_registry.rs (sdk-rs/target/debug/deps/postprocessor_registry-be4f67ea7bd49b3c)

running 1 test
test constructor_lists_keep_order_identity_and_live_mutation ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
```

### registry-merge-check output

```text
    Checking safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 26s
```

### registry-merge-runtime output

```text
    Blocking waiting for file lock on package cache
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 11.01s
     Running tests/dev.rs (sdk-rs/target/debug/deps/dev-f9a1d5ca7a805125)

running 1 test
test dev_dispatch_reaches_real_socket_and_regional_how_targets ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.67s

     Running tests/message_stats.rs (sdk-rs/target/debug/deps/message_stats-2e26d152f90a181d)

running 1 test
test message_stats_match_live_socket_precedence_and_pin_shared_representation_gaps ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s

     Running tests/postprocessor_registry.rs (sdk-rs/target/debug/deps/postprocessor_registry-be4f67ea7bd49b3c)

running 1 test
test constructor_lists_keep_order_identity_and_live_mutation ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s

     Running tests/twap_state.rs (sdk-rs/target/debug/deps/twap_state-2e06290632c055fe)

running 2 tests
test source_market_mutation_cannot_rewrite_the_captured_input ... ok
test new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.47s
```

### registry-merge-timers output

```text
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 25.05s
     Running unittests src/lib.rs (sdk-rs/target/debug/deps/safe_cex_webworker-b834bbb591838939)

running 2 tests
test subscriptions::tests::immediately_discarded_timeout_preserves_synchronous_leading_updates ... ok
test subscriptions::tests::discarded_stream_timers_release_the_slot_and_drop_does_not_retain_the_owner ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

### registry-merge-extract-check output

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
Done in 12.48s.
```

## Final merge before push

Fetched and merged reviewed integration `55d8addaf74e5ff2c2da80ab19a03109b6fb9476` to merge SHA `2b164e4108e2755cfe36ec83f4f5234973694749`. `git diff --exit-code 697440bed8 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0**. The worker and its dependency sources are byte-unchanged; the lockfile only adds dependencies to Coinbase/Weex, outside this worker graph. The nine runtime tests at 697440bed8 and two timer tests at 2bd1d02802 still apply.

Re-ran scoped cargo check and extractor at 2b164e4108; both measured **exit 0**. Extractor output: 7341 symbols, zero updated/added/orphaned. Python3.12 tomllib re-count against final integration 55d8addaf7 confirmed 103/57/62 →104/59/59: one pending→ported, two→divergent. No CLAUDE conflict occurred. The final evidence commit changes only this tracked sheet.

### registry-delivery-check output

```text
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Checking safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 06s
```

### registry-delivery-extract-check output

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
Done in 15.81s.
```
