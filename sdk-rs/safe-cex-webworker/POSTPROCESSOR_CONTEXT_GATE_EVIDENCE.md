# Postprocessor context gate evidence

Moved **6 pending → ported, 0 → divergent**. Initial implementation parent: `5ffc94fa620b26e96d54b08104a547ec76408dea`. Initial measured code SHA: `77af5ef79b299dc258f63786ba7c96e3661a304c`. Merged reviewed integration `54851c81b7c780ebfea3682883bd3a49e12b9d8f`; refreshed gates measured at `64af066d631caeb4fe227eb3876dd0c8808ea870`. No unreviewed feature branch is an ancestor outside this slice. The final documentation commit changes this evidence file only; the later integration merge is recorded below.

## Native proof and scope

The actual two TypeScript abstract-base constructors are transpiled and executed. Rust constructors retain their supplied opaque slots directly, and the abstract methods become required trait methods with no default body. The fixed expectation is two declarations and eight cases: independent/dependent × ASCII, astral Unicode, empty, lone-surrogate UTF-16. Enumeration stopped after dependent case 3 (eighth case). This is a coverage floor, not proof of all possible carriers, subclass reflection/prototypes, or the complete worker pipeline.

The Rust test decodes options from independently fixed JSON and checks retained native buy/sell binary64 bits, UTF-16 account units, exchange name, option/exchange reference identity, live caller mutations, negative zero, and concrete override dispatch before comparing profiles with the source. Profiles contain native observations, not decode/re-encode roundtrips. Counts are literal independent expectations (2 declarations, 8 cases).

## Measured gates

All oracle cargo test commands used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Cargo commands ran sequentially with `--jobs 2`.

| SHA | Gate | Exit |
| --- | --- | --- |
| `77af5ef79b` | `context-doc-clean` | 0 |
| `77af5ef79b` | `context-runtime-clean` | 0 |
| `77af5ef79b` | `context-base-required-mutant` | 101 |
| `77af5ef79b` | `context-base-required-restored` | 0 |
| `77af5ef79b` | `context-dependent-required-mutant` | 101 |
| `77af5ef79b` | `context-dependent-required-restored` | 0 |
| `77af5ef79b` | `context-base-constructor-mutant` | 101 |
| `77af5ef79b` | `context-base-constructor-restored` | 0 |
| `77af5ef79b` | `context-dependent-constructor-mutant` | 101 |
| `77af5ef79b` | `context-dependent-constructor-restored` | 0 |
| `77af5ef79b` | `context-scoped-check` | 0 |
| `64af066d63` | `context-merge-check` | 0 |
| `64af066d63` | `context-merge-tests` | 0 |
| `64af066d63` | `context-merge-doc` | 0 |
| `77af5ef79b` | extractor check | 0 |
| `64af066d63` | refreshed extractor check | 0 |
| `24b947b979` | final merge scoped check | 0 |
| `24b947b979` | final merge extractor check | 0 |

Exact clean commands:

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test postprocessor_context --locked --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --doc postprocessor_context --locked --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test postprocessor_context --test store_payload --test twap_manager --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

The writer `parity:extract` ran after structured manifest serialization before the implementation commit; its output was 7341 symbols / 512 source files, 0 updated, 0 added, 0 orphaned. All three measured extractor checks reported 7341 symbols and “Would update 0 manifests; 0 added, 0 orphaned symbols.” Output text was inspected as well as measured exits.

## Discrimination

Each mutation was an uncommitted change relative to the initial measured SHA, restored exactly before its clean rerun. No mutation survivors were observed in these four controls; no larger mutation inventory is claimed.

| Mutated call site | Named failure | Mutant / restored exits |
| --- | --- | --- |
| `BasePostprocessorContext::new` discards supplied options for `Options::default()` (temporary `Default` bound) | `CONTEXT native constructor buy independent/0`: native 0 instead of bits 4608308318706860032 (1.25) | 101 / 0 |
| `ExchangeDependentPostprocessorContext::new` discards supplied options identically | `CONTEXT native constructor buy dependent/0`: native 0 instead of 1.25 bits | 101 / 0 |
| `BasePostprocessor::process` acquires a default no-op | its omitted-override doctest: “Test compiled successfully, but it’s marked compile_fail.” | 101 / 0 |
| `ExchangeDependentPostprocessor::process` acquires a default no-op | its omitted-override doctest: same unexpected compilation | 101 / 0 |

## Merge survival and review

`git diff --exit-code 77af5ef79b HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured exit **1** at `64af066d63`. Integration added TwapManager/source registration and changed the store-payload ticker fixture, plus changed dependency sources. Therefore scoped compilation, selected runtime tests, targeted doctests, and extractor were refreshed at the merge SHA.

The narrower diff covering this slice’s `postprocessor_context.rs`, context test/oracle, two manifests, and both source TypeScript bases measured **exit 0** across the merge. Constructor/requiredness mutation inputs stayed byte-unchanged; the fresh clean runtime/doc runs passed, so the unchanged controls were not repeated. No CLAUDE conflict occurred; both independently added paragraphs remain.

Sub-agent `context_review` performed two read-only reviews and ran no gates. It found no blockers in the six mappings or implementation. Its first review identified that positive override dispatch alone would not discriminate an accidental default no-op; the two negative compile witnesses and measured mutations resolve that gap.

## Census

Python 3.12 `tomllib` parsed all 38 tracked webworker manifest files and compared symbol identities/statuses to each explicit parent. Enumeration stopped at manifest file 38, 222 rows total. Against initial parent: 97 ported / 50 divergent / 75 pending → 103 / 50 / 69. Against merged integration: 97 / 57 / 68 → **103 / 57 / 62**. Seven additional divergent rows came from integration’s TwapManager slice and are not claimed as this slice’s work. Exactly six matching symbol identities changed pending → ported here.

## Not run

No workspace-wide compilation, full monorepo suite, host clippy, wasm clippy, browser/wasm worker runtime, live venue tests, or CI rerun. This sheet reports only the scoped commands above.

### context-merge-check output

```text
    Checking safe-cex-types v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-types)
    Checking safe-cex-num v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-num)
    Checking safe-cex-crypto v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-crypto)
    Checking safe-cex-candle-cache v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-candle-cache)
    Checking safe-cex-store v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-store)
    Checking safe-cex-transport v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-transport)
    Checking safe-cex-exchange v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-exchange)
    Checking safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 21s
```

### context-merge-tests output

```text
   Compiling safe-cex-types v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-types)
   Compiling safe-cex-num v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-num)
   Compiling safe-cex-crypto v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-crypto)
   Compiling safe-cex-store v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-store)
   Compiling safe-cex-candle-cache v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-candle-cache)
   Compiling safe-cex-transport v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-transport)
   Compiling safe-cex-exchange v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-exchange)
   Compiling safe-cex-webworker v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-p6-webworker/sdk-rs/safe-cex-webworker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 22.77s
     Running tests/postprocessor_context.rs (sdk-rs/target/debug/deps/postprocessor_context-d0aff6cab4dd2ed7)

running 1 test
test abstract_contexts_preserve_inputs_and_dispatch_required_overrides ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s

     Running tests/store_payload.rs (sdk-rs/target/debug/deps/store_payload-31f93a538c9758cf)

running 1 test
test snapshot_bookkeeping_matches_live_versions_and_ticker_transitions ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s

     Running tests/twap_manager.rs (sdk-rs/target/debug/deps/twap_manager-e58cbcdca3ea395b)

running 1 test
test manager_membership_order_and_live_fields_match_executed_source ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
```

### context-merge-doc output

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.41s
   Doc-tests safe_cex_webworker

running 2 tests
test safe-cex-webworker/src/postprocessor_context.rs - postprocessor_context::BasePostprocessor (line 23) - compile fail ... ok
test safe-cex-webworker/src/postprocessor_context.rs - postprocessor_context::ExchangeDependentPostprocessor (line 68) - compile fail ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
```

### context-merge-extract-check output

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
Done in 21.60s.
```

## Final merge before push

Fetched and merged reviewed integration `f22d7b2c94b74e1f29f96d28b6d07c3163bb5409` to merge SHA `24b947b979475b7868e8adcccc8f6e7987adf78f`. This brought Backpack/GRVT work and added only GRVT-local dependencies to Cargo.lock. No dependency source used by this worker changed. No CLAUDE conflict occurred.

`git diff --exit-code 64af066d63 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **exit 0** at `24b947b979`: the crate source and tests are byte-unchanged from their latest runtime/doctest gate SHA. Those unchanged runtime/doc gates stand. The broader repository diff excluding this evidence measured exit 1 for the unrelated integration work; no repository-wide unchanged claim is made.

Re-ran the exact scoped cargo check and extractor check at `24b947b979`; both measured **exit 0**. The extractor again reported 7341 symbols / 512 files and zero updated, added, or orphaned rows. Python 3.12 tomllib re-count against final integration `f22d7b2c94` independently confirmed 97/57/68 → 103/57/62: six pending → ported, zero → divergent. No unreviewed external feature work is an ancestor.

### context-final-merge-check output

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.63s
```

### context-final-merge-extract-check output

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
Done in 13.43s.
```
