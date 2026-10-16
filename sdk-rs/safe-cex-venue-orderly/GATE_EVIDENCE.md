# Orderly websocket control evidence

Parent: `0e116b60e90e37e08f3995645493fc20176c0b86` (integration; no unreviewed
feature ancestor). Gate/review snapshot: `7c38efb7d11e1bd379c6bcd53f86326a056aec43`.
`tomllib` measures **10 pending -> ported, 0 -> divergent** against the parent.
Orderly census: **23 ported / 16 divergent / 56 pending**, 95 total.

Ten rows: public/private connectAndSubscribe, onOpen, subscribe and ping;
public onMessage and handlePingEvent. Complete bodies retain original opaque
receiver/value/callback/error handles and current runtime member lookup. Imported
and intrinsic dependencies remain explicit in `WsHost`; classes remain pending.
Canonical scope: `sdk-rs/parity/MANIFEST-SPEC.md#orderly-websocket-control-bodies`.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test ws_controls --jobs 2`: **0**,
  ten named tests / 62 finite cases. Floors: 11 connect, 12 open, 11 subscribe,
  16 ping/pong and 12 message. Native field shape/order, UTF-16, number bits/NaN,
  original socket/callback/parsed/error identity and effects precede encoding.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/ws-controls-controls.py`: **0**.
  Eight independently compiled defects: network selection, public open topic,
  private auth key, public flatten bypass, private subscription topic, shared
  ping event, public pong event and disabled public dispatch. Each produces
  **101** at its required named test(s); restored target produces **0** for all
  ten names. Exact source bytes restored. No survivors. Nested bodies are tested
  with separate defects so one mutant cannot mask another's independent gate.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER `parity:extract`: **0**, updated 2 manifests, added 0,
  orphaned 0. `rust_files` derived from row targets with `tomllib` before writer.
- Explicit Yarn `parity:extract:check`: **0**, 7341 symbols / 512 files;
  would update 0 manifests, added 0, orphaned 0.
- Read-only sub-agent review: clear at the full snapshot; independently measures
  the same 10 ported movement and 23/16/56 census. No sub-agent Cargo or edits.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser or live exchange tests.
No shared SDK type or trait changed. Controlled stringify returns its original
argument for native payload checks; this is no wire/JSON codec gate. Parser,
signer, clocks and base effects are controlled dependencies, not fresh crypto,
nonce, promise or network integration gates. Exotic accessors/proxies, sparse
arrays, non-string includes, general JSON/scheduling and fixed class assembly
are outside this finite witness.

## Merge preservation

Integration `acf29148c3f26a07b72f93d1d964b49e81016ec6` merged down as
`d038e61c79158ba795bf2235cd01c407d230e5ae`, without conflicts. Mandatory standalone

```
git diff --exit-code 7c38efb7d11e1bd379c6bcd53f86326a056aec43 HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**: production and native/oracle witnesses are byte-unchanged.
Additional gate-input scope (controls, Orderly manifests/canonical, SDK shared
dependencies, Cargo/lock, extractor/Yarn/package scripts and relevant TypeScript)
has no changed paths. Cargo, mutation and review exits therefore still apply;
those unchanged gates were not rerun after merge. Other venue manifests changed,
so extractor check reran at the merge SHA: **0**, 7341/512, would update 0,
added 0, orphaned 0. `tomllib` against fetched integration still measures
**10 pending -> ported, 0 -> divergent**, census **23 / 16 / 56**.
This record is evidence-only; standalone source/test diff is repeated before push.

## Stream and report bodies

Parent: `c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`eea659185784674de2068294f35074f724684597`. `tomllib` against the parent measures
**8 pending -> ported, 0 -> divergent**; census **31 / 16 / 48**, 95 total.

Rows: four public ticker/BBO/mark/funding handlers; private message dispatch,
position, execution and algo execution reports. Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#orderly-stream-and-report-bodies`.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test stream_reports --jobs 2`:
  **0**, eight named tests / 86 cases (32 public, 16 dispatch, 8 position,
  15 execution, 15 algo). Native field/key shape, UTF-16, numeric bits/NaN and
  original row/mapper/ticker/store/error identity precede encoding. Native trace
  snapshots copy values after identity checks, without an encoding roundtrip.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/stream-reports-controls.py`:
  **0**. Eight disjoint compiled defects cause **101** in all eight required
  named native assertions; restored target returns **0**, exact source bytes.
  Defects: ticker close, bid, mark, funding input, private ping response,
  position addition guard, execution amount, algo amount. Each named case
  discriminates its own defect before any cross-body integration case. No survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 2 manifests, added 0, orphaned 0;
  target-derived `rust_files` computed with `tomllib` before writer.
- Explicit Yarn extractor check: **0**, 7341 symbols / 512 files; would update 0
  manifests, added 0, orphaned 0.
- Read-only sub-agent review: clear at the full gate snapshot; independently
  measures the same eight ported movement and 31/16/48 census. No agent Cargo/edits.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. JSON/base/mappers remain controlled dependencies;
no general codec, fresh crypto/nonce, promise/network or fixed class/mapping
integration claim. Exotic accessors/proxies, sparse/custom array methods and
malformed/general JSON remain outside the finite floor. Native imported bindings
cover declared values; this is not a fresh gate of already-resolved import rows.

## Stream/report merge preservation

Integration `cdf2af4e594ab1eaea08f3e6811e2d142c3f0dd1` merged down as
`c5e9f7b81a3f962db3c22acf105952d9c34ee60b`, no conflicts. Standalone

```
git diff --exit-code eea659185784674de2068294f35074f724684597 HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Production and native/oracle witnesses are byte-unchanged.
The additional input scope (controls, Orderly manifests/canonical, SDK dependency
crates, Cargo/lock, extractor/Yarn and relevant TypeScript) has no changed paths.
Only root package.json's KuCoin test target adds public_streams; parity scripts
are unchanged. Cargo/mutation/review exits therefore stand and were not repeated.
Other venue manifests changed, so extractor check reran at the merge SHA:
**0**, 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
`tomllib` against fetched integration measures **8 pending -> ported, 0 -> divergent**;
census **31 / 16 / 48**. This preservation commit changes only tracked evidence;
standalone source/test diff is repeated before push.

## Account and order REST read bodies

Parent: `74fc4846d408b2b29e843728c4233fe900abb371`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`b0fc7e2558d5e5627ba7534231da7f5efc25e5db`. `tomllib` against that parent
measures **8 pending -> ported, 0 -> divergent**; census **39 / 16 / 40**, 95 total.

Rows: position mode, account, validation, leverage and margin-type updates,
limit/algo/combined order reads. Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#orderly-account-and-order-rest-read-bodies`.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test account_order_reads --jobs 2`:
  **0**, eight named tests / 71 finite cases: 8 mode, 8 account, 8 validation,
  9 leverage, 8 margin, 12 limit, 9 algo and 9 combined. Native value/field shape,
  numeric bits/NaN, UTF-16, original opaque row/result/error identities and
  synchronous prefixes/continuations precede encoding. Primitive undefined uses
  source value equality; opaque objects retain pointer identity assertions.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/account-order-reads-controls.py`:
  **0**. Eight compiled body-local defects produce **101** in all eight required
  named native assertions, followed by restored **0** for all eight names and
  exact source restoration. Defects: mode value, affiliate truthiness, validation
  msg/message, leverage fallback, margin enum filter, null-only limit filtering,
  algo children omission and combined result order. Independent body cases
  discriminate before combined integration cases; no survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 1 manifest, added 0, orphaned 0. Target
  union for rust_files was derived with tomllib before the writer.
- Explicit Yarn extractor check: **0**, 7341 symbols / 512 files;
  would update 0 manifests, added 0, orphaned 0.
- Read-only sub-agent review: clear at the full gate snapshot; independently
  confirmed 8 ported movement and 39/16/40 census. No agent Cargo or edits.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. XHR/await engine and mappers are controlled
bindings; no full Promise/thenable/HTTP/class/mapping assembly, signing/nonce or
live integration claim. Exotic accessors, sparse/custom arrays, malformed
response structure and general scheduling remain outside the finite floor.
Source imports evaluated here are dependencies, not fresh gates of their old rows.

## Account/order read merge preservation

Integration `500345147d6c0d4671f1b7382b1d83902fdcf3fb` merged down as
`1927a72552588906901a147905defb9dad44d7d4`, no conflicts. Standalone

```
git diff --exit-code b0fc7e2558d5e5627ba7534231da7f5efc25e5db HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Source and native/oracle witnesses are byte-unchanged.
Additional input scope has no changes except Cargo.lock adding safe-cex-crypto
to Bitget's dependency list. A tomllib traversal from safe-cex-venue-orderly,
resolving every dependency name/version, verifies all **437** selected transitive
package records are identical before/after merge. Thus Orderly's lock dependency
closure and code/config/import inputs are unchanged; Cargo/mutation/review exits
stand and were not repeated after merge.

Other venue manifests changed, so extractor check reran at the merge SHA:
**0**, 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
`tomllib` against fetched integration still measures **8 pending -> ported,
0 -> divergent**, census **39 / 16 / 40**. This preservation record changes only
tracked evidence; the standalone source/test diff is repeated before push.

## Cancellation, leverage, margin and algo edit actions

Parent: `f2b6e51c1a29d354ad93b5602fbc44227d9714c5`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`6672a436f91381fe147437f2c4099e67d8031494`. `tomllib` measures
**8 pending -> ported, 0 -> divergent**; census **47 / 16 / 32**, 95 total.
Canonical scope: `sdk-rs/parity/MANIFEST-SPEC.md#orderly-trading-action-bodies`.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test trading_actions --jobs 2`:
  **0**, eight named tests / 76 cases (8 edit, 8 serial cancel, 9 algo cancel,
  7 bulk, 7 symbol cancel, 14 single leverage, 13 all leverage, 10 margin).
  Native results, UTF-16, number bits/NaN, effects/payload/state and original
  positions/error handles are asserted before encoding. Actual source arrows
  in five SDK source files plus installed forEachSeries are hashed (six files).
  Append during serial callback exercises live series length and its second
  request; await changes exercise current XHR, sibling price and position state.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/trading-actions-controls.py`:
  **0**. Seven disjoint compiled family defects produce **101** in all eight
  required named assertions; restored target returns **0**, exact source bytes.
  Defects: edited price, cancel ID radix, sibling price key, shared bulk endpoint,
  leverage rounding, all-leverage margin mode, margin payload key. The shared
  endpoint control requires both bulk and symbol named failures. No survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 1 manifest, added 0, orphaned 0.
  rust_files union derived from row targets with tomllib before writer.
- Explicit Yarn extractor check: **0**, 7341 symbols / 512 files;
  would update 0 manifests, added 0, orphaned 0.
- Read-only sub-agent review: clear at full gate snapshot, independently measures
  eight ported and 47/16/32; no agent Cargo or edits. Initial vacuous append label
  was corrected to an actual append effect before freezing.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`.
A development preflight stalled loading a cached macro dylib (sampled loader
fcntl), then was interrupted at **143**; it supplies no gate evidence. The
subsequent real baseline and frozen mutation/restoration gates passed above.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live trading tests.
No shared SDK type/trait changed. HTTP/Promise/Error/array/import bindings are
controlled dependencies, not fixed class/runtime/network/signing/nonce assembly.
Exotic accessors, sparse/custom arrays, promise-valued series entries, arbitrary
coercion/iterators and general scheduling remain outside the finite witness.

## Trading-action merge preservation

Integration `7e9e5c660472d985e231394357b25ed171b929f2` merged down as
`f14de6279e3e2d6a0bb61b33ef88b044ecad2a78`, without conflicts. Standalone

```
git diff --exit-code 6672a436f91381fe147437f2c4099e67d8031494 HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Source and native/oracle witnesses are byte-unchanged. Additional
input scope (controls, Orderly manifests/canonical, SDK dependency crates,
Cargo/lock, extractor/Yarn/package and relevant TypeScript) has no changed paths.
Cargo/mutation/review exits still stand; those unchanged gates were not repeated.
Other venue manifests changed, so extractor check reran at the merge SHA: **0**,
7341/512, would update 0, added 0, orphaned 0. `tomllib` against fetched integration
still measures **8 pending -> ported, 0 -> divergent**, census **47 / 16 / 32**.
This commit changes only tracked evidence; standalone source/test diff is repeated
before push.

## Data reads, mappers and subscription delegates

Parent: `22ee2d3fdd6e60b617e95d3179571adf0111590b`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`f90512e8746756edfe37e603b97b361f109f8f3e`. Recursive `tomllib` census:
**9 pending -> ported, 0 -> divergent**; **47 / 16 / 32 -> 56 / 16 / 23**,
95 total (includes six pending derived-venue classes). Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#orderly-data-and-mapper-bodies`.

Measured gates:

- Scoped `cargo test -p safe-cex-venue-orderly --test data_mappers --jobs 2`:
  **0**, nine named tests / 66 cases (7 markets, 8 tickers, 11 positions,
  11 position mapper, 7 limit mapper, 10 algo mapper, 6 candles, 3 per delegate).
  Native results/effects/current balance, UTF-16, number bits/NaN, original
  raw-info/cache/error/callback/cleanup handles and returned cloned info roots
  plus nested fields are asserted before encoding. Actual source declarations
  in six SDK files are hashed. One positions case integrates the real mapper;
  post-await cases change current market/balance/leverage state.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/data-mappers-controls.py`:
  **0** at gate snapshot. Nine disjoint compiled family defects produce **101**
  in all nine required named assertions; restored target returns **0**, exact
  source bytes restored. Defects: market active flag, ticker bid, balance used
  operator, position contracts, limit raw info, algo price, candle timestamp
  divisor and each delegate callback. No survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 1 manifest, added 0, orphaned 0;
  rust_files union derived from row targets with tomllib before writer.
- Explicit Yarn extractor check: **0**, 7341 symbols / 512 files;
  would update 0, added 0, orphaned 0. It started at implementation SHA
  `67f923f2fbce1ab66cf260d1547dd6848cf45068`; a standalone scoped diff from
  that SHA to the gate SHA returned **0** for source/tests, all manifests,
  canonical notes, extractor, package.json and yarn.lock. The sole intervening
  commit fixes mutation-script compilability. Extractor inputs are unchanged.
- Read-only sub-agent review: clear at full gate SHA; independently measured
  nine ported and census 56/16/23. No agent Cargo or edits. Initial review
  findings (spread timing, native order-type binding and returned identity
  witnesses) were resolved before freezing.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`. Development preflights failed
compilation (Rc dereference and unused import); the first control attempt also
failed compilation on unused mutant variables. None earns discrimination credit.
The corrected compiled failures and restoration above supply the gate evidence.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. HTTP/Promise/Error/array/import bindings are
controlled dependencies; no fixed class/runtime/network/signing/nonce assembly
claim. Exotic accessors/proxies, sparse/custom arrays, arbitrary coercions/
iterators, malformed response structures and general scheduling remain outside
the finite floor. Evaluated source imports are dependencies, not new row gates.

## Data-body merge preservation

Integration `f6c865ddd8331c32516f063d90d045761bd2c73b` merged down as
`7231f559f23914688268ec69e9df66f96c741831`, without conflicts. Standalone

```
git diff --exit-code f90512e8746756edfe37e603b97b361f109f8f3e HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Production and witnesses are byte-unchanged. Additional scoped
inputs (controls, Orderly manifests/canonical, SDK crates, Cargo manifests,
extractor/Yarn/package and relevant source TypeScript) have no changed paths
except Cargo.lock adding base64/safe-cex-crypto to Blofin. A tomllib traversal
resolves the entire Orderly dependency closure and compares all **437** selected
package records: identical. Another scoped diff verifies source/test/script/
Cargo.toml inputs of all **5** selected local crates unchanged, exit **0**.
Thus Cargo/mutation/review gates still stand and were not repeated after merge.

Other venue manifests changed, so extractor check reran at the merge SHA: **0**,
7341 symbols / 512 files, would update 0, added 0, orphaned 0. Recursive tomllib
against fetched integration still measures **9 pending -> ported, 0 -> divergent**,
census **56 / 16 / 23**, 95 total. This preservation commit changes tracked
evidence only; the standalone source/test diff is repeated before push.

## Runtime orchestration and order execution

Parent: `fc23eb289f39c11f1bf30b56f1848254b2a520ac`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`5a775ae118baff3fb4dde72fa9f8a6d82c89d0f8`. Recursive tomllib measures
**8 pending -> ported, 0 -> divergent**, census **56 / 16 / 23 -> 64 / 16 / 15**,
95 total. Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#orderly-runtime-and-order-execution-bodies`.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test runtime_actions --jobs 2`:
  **0**, eight named tests / 110 cases (10 transient classification, 4 disposal,
  16 startup, 16 polling, 25 amendment, 18 placement, 10 algo, 11 positional).
  Native UTF-16, numeric bits/NaN, original input/delegated/error/background-
  promise identities, synchronous prefixes and ordered results/effects/current
  state are asserted before encoding. Actual source declarations in eight SDK
  files plus installed p-iteration are hashed, nine source files.
  Focused cases constrain later-getter suppression after NaN precision, detached
  loop rejection versus propagated interval failure, background catch fallback
  and logger failure, current tick resolution/count concatenation, disposal
  checkpoints, post-await XHR/options changes, serial lots and zero-fill guards.
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/runtime-actions-controls.py`:
  **0**. Eight disjoint compiled family defects yield **101** in all eight
  required named native assertions; restored target returns **0**, exact source
  bytes restored. Defects: transient threshold, disposal flag, loaded-market
  flag, polling PnL divisor, amend payload key, placement tag, algo stop key and
  positional algo type. No survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 1 manifest, added 0, orphaned 0;
  rust_files union derived from row targets with tomllib before writer.
- Explicit Yarn extractor check: **0**, 7341 symbols / 512 files, would update
  0, added 0, orphaned 0.
- Read-only sub-agent review: clear at full gate SHA; independently measured
  eight ported, census 64/16/15 and finite floor 110. No agent Cargo or edits.
  Guard-read order and detached scheduling findings were fixed before freezing.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`. First development preflight failed
compilation on a duplicate yield helper; it earns no gate credit. The corrected
baseline and frozen mutation/restoration gates above passed.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. Runtime/array/import/error/HTTP/Promise/loop
contracts are bindings; no fixed class, generic event-loop/timer/thenable engine,
network/signing/nonce assembly or live trading claim. Apart from the one focused
precision getter, exotic accessors/proxies, sparse/custom arrays, arbitrary
coercions/iterators and malformed response structures remain outside the finite
floor. Source imports are dependencies, not fresh gates of older rows.

## Runtime/action merge preservation

Integration `5c3c55b7b5631e9d04b3d31bc98b213b5888f18d` merged down as
`0985db87942e778c90eedb9b9124b2a536d84571`, without conflicts. Standalone

```
git diff --exit-code 5a775ae118baff3fb4dde72fa9f8a6d82c89d0f8 HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0** before and after gate restoration. Body sources, witnesses,
controls, mappings and this unit's canonical section remain byte-unchanged;
read-only review confirms preservation at the merge SHA. Other selected inputs
have no changes except crypto's added Poseidon modules/tests and Cargo.lock
changes for other venues. Tomllib resolves and compares all **437** selected
Orderly lock dependency records: identical. The crypto source changed, so
compiled gates were rerun at the merge SHA, rather than carried forward:

- Runtime controls: **0**, eight required named compiled failures **101**,
  restored target **0** (eight tests / 110 cases), exact source bytes restored.
- Scoped crate `cargo check --all-targets --jobs 2`: **0**.
- Extractor check: **0**, 7341 symbols / 512 files, would update 0, added 0,
  orphaned 0. This rerun also covers the changed other-venue manifests.

Orderly formatting source/config inputs did not change; the scoped fmt **0**
stands and was not repeated. Cargo/formatter/toolchain config diff has no paths.
Recursive tomllib against fetched integration still measures **8 pending ->
ported, 0 -> divergent**, census **64 / 16 / 15**, 95 total. This record changes
tracked evidence only; source/test preservation diff is repeated before push.

## Runtime/action pre-push integration check

Fetched integration again immediately before push:
`ff33a3d2b187e596e1c58511c3bf35b6306d4d7b`, merged down as
`f8fb6cc063d7faf43f4cb0645921be842d78ae89`, no conflicts. Standalone
source/test preservation diff against reviewed `5a775ae118baff3fb4dde72fa9f8a6d82c89d0f8`
returns **0**. All additional selected input scopes against rerun snapshot
`0985db87942e778c90eedb9b9124b2a536d84571` have no changed paths, including
Cargo.lock, selected crypto and other SDK code, control script, canonical/Orderly
manifests, source TypeScript, extractor, package/Yarn and Cargo/formatter configs.
The compiled native gates/check and formatting exits still stand. Only another
venue's manifest changed, so extractor check reran at the new merge SHA: **0**,
7341/512, would update 0, added 0, orphaned 0. Recursive tomllib still measures
**8 pending -> ported, 0 -> divergent**, census **64 / 16 / 15**, 95 total,
against the latest fetched integration. This record changes evidence only;
standalone source/test diff is repeated after this commit and before push.

## Exchange constructor and derived class factories

Parent: `82cf7c04c74535f5bf327375759919ec143ee868`, integration with no
unreviewed feature ancestor. Gate/review snapshot:
`b2d430af6c7e58336eb640cd6e5114420b06d8b0`. Recursive tomllib measures
**8 pending -> ported, 0 -> divergent**, census **64 / 16 / 15 -> 72 / 16 / 7**,
95 total. Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#orderly-exchange-class-factories`.

This unit is an adapted constructor/initializer/composition factory surface.
Complete original-source ABIs for all 39 members and superclass/API/socket
construction are mandatory external dependencies. It does not ship a fixed
complete runtime class, reclassify inherited member divergences, or supply new
invocation-adapter coverage for the 37 uninvoked members. Member provisioning
is pure/infallible; prototype installation must preserve the first registration.

Measured gates at the snapshot:

- Scoped `cargo test -p safe-cex-venue-orderly --test classes --jobs 2`: **0**,
  eight named tests / 60 cases (11 constructor, 7 per class for seven classes).
  Actual unchanged source class declarations plus rule/types helpers in nine
  SDK files are hashed. Independent floors: 49 definitions, 48 own fields, 38
  arrows, 1 prototype, 54 instance fields including six controlled base fields.
  Native descriptor flags/key order/values/UTF-16/number bits, original error
  metadata/identity, shallow option sharing, fresh arrows/Maps/rules, shared
  imports, persistent prototype body and original caller options are asserted
  before encoding. Source constructor import failures, normalization and
  derived newTarget/tag timing have paired native witnesses. The two invoked
  native chains are detached candle delegation (original arguments/cleanup)
  and cached prototype position mode (foreign receiver/original returned mode).
- `python3.12 sdk-rs/safe-cex-venue-orderly/scripts/classes-controls.py`: **0**.
  Eight disjoint compiled family defects yield **101** in all eight required
  named native UTF-16 assertions; restored target returns **0**, exact source
  bytes restored. Defects: constructor prefix, core factory tag and six derived
  tags. Each class's first case has prefixed credentials, so the common
  constructor-prefix defect cannot mask that class's tag defect. No survivors.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-orderly --check`: **0**.
- Explicit Yarn WRITER: **0**, updated 7 manifests, added 0, orphaned 0; rust_files
  union derived from row targets with tomllib before writer.
- Explicit Yarn extractor check: **0**, 7341 symbols/512 files; would update 0,
  added 0, orphaned 0.
- Read-only sub-agent review: clear at full gate SHA for the scoped adapted
  factories; independently measured 8 ported, 72/16/7, 60 cases and 37 external
  uninvoked member dependencies. No agent Cargo or edits. Prototype persistence
  and factory boundary scope were made explicit before freezing.

Cargo used `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`;
tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js` with
`parity:extract` and `parity:extract:check`. Initial preflight failed compilation
on an unused import and omitted Result unwrap; it earns no gate credit. Frozen
mutation/restoration runs above were sequential and restored exact bytes.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. Fixed complete member/HTTP/websocket/signer/
runtime/collector assembly, other 37 member invocations, exotic accessor/proxy
behavior, arbitrary option types, GC and foreign-runtime inheritance are outside
this finite floor. The class-factory row does not regate already-resolved members.

## Exchange factory merge preservation

Integration `8b3442af01690851ec63406f14e7cca587959060` merged down as
`a3ef9ac8b059270dc9e71b743d35f9c224d3d3c4`, without conflicts. Standalone

```
git diff --exit-code b2d430af6c7e58336eb640cd6e5114420b06d8b0 HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Sources and witnesses are byte-unchanged. Additional selected
input scopes have no changes except Cargo.lock adding futures-util to Woo.
A tomllib graph traversal resolves and compares all **437** selected Orderly
lock records: identical. A scoped diff checks source/test/script/Cargo.toml
inputs of all **5** selected local crates unchanged, exit **0**. Controls,
Orderly manifests/canonical, relevant source TypeScript/client-rule helper,
extractor/Yarn/package and compiler/formatter configs are unchanged. Therefore
Cargo/mutation/review/fmt exits stand and were not repeated after merge.

Other venue manifests changed, so extractor check reran at the merge SHA: **0**,
7341 symbols / 512 files, would update 0, added 0, orphaned 0. Recursive tomllib
against fetched integration still measures **8 pending -> ported, 0 -> divergent**,
census **72 / 16 / 7**, 95 total. This record changes tracked evidence only;
standalone source/test diff is repeated after committing it and before push.

## API, public feeds and websocket initializer factories

Actual parent: `e8fea3f53ada0bda88ee35193548bbbdb9f4063d`.
Frozen gate SHA: `45dae1110151feb503e2e7b16fd4cc863123e99e`.
Recursive tomllib comparison against that parent moves **7 pending -> ported,
0 -> divergent**: **72 / 16 / 7 -> 79 / 16 / 0**, 95 Orderly rows including
six derived exchanges. The rows are signRequest, createAPI, public/private
websocket classes, listenOHLCV, listenOrderBook and processOrderBookUpdate.

Measured gates on the frozen implementation:

- Scoped target `cargo test -p safe-cex-venue-orderly --test api_book_sockets
  --jobs 2 --locked`: **0**, seven named tests / **41** finite cases. Independent
  floors: signing 9, API 8, candles 6, book 9, delta processing 3, public factory
  3 and private factory 3. Actual unchanged source declarations/helpers from
  **7** SDK files are hashed. Native ordered values/keys/UTF-16/number bits/NaN,
  effects, mutations, original errors and callback book sharing are asserted
  before encoding. Source/native paired cases include query insertion versus
  sorted serialization, public config identity, current mutable options/broker,
  throwing URL conversion before query, buffered timestamp filtering, REST retry,
  synchronous request failure, callback/logger failure and cleanup while pending.
- `python3.12 .../scripts/api-book-sockets-controls.py`: **0** at the full frozen
  SHA. Seven disjoint compiled defects each hit its required named native
  assertion, aggregate **101**; exact bytes restored and all seven named tests
  return **0**. Defects: signing content header, retry count, candle time divisor,
  snapshot strict timestamp filter, delta initial total, public topics field and
  private ping field. First API case is public, so signing cannot mask retry
  discrimination; delta initial totals are recalculated in the book family.
  **No survivors.** Standalone working-source/test diff against gate returns **0**.
- Scoped `cargo check -p safe-cex-venue-orderly --all-targets --jobs 2 --locked`:
  **0** on restored frozen source. Scoped `cargo fmt -p ... --check`: **0**.
- Explicit Yarn WRITER `parity:extract`: **0**, updated 0 manifests, added 0,
  orphaned 0. rust_files arrays were first derived from row target unions via
  tomllib. Explicit Yarn `parity:extract:check`: **0**, 7341 symbols / 512 files,
  would update 0 manifests, added 0, orphaned 0.
- Read-only sub-agent review at full frozen SHA: **clear**. Independently counted
  seven moved rows, census 79/16/0 and 41 cases; reviewed source semantics,
  native assertions and dependency/factory boundaries. No agent Cargo or edits.
  URL conversion order was corrected with a paired witness before freezing.

Cargo path: `/opt/homebrew/opt/rustup/bin/cargo`; manifest `sdk-rs/Cargo.toml`.
Tests use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn is `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

Class floors are 15 public and eight private initialized own fields, plus one
controlled superclass field; descriptor flags/key order, fresh arrows/topics/
handler arrays and original parent arguments are checked. The six public
handler wrappers dispatch current replaced members on the original receiver
with original data. All 21 member ABIs are mandatory external dependencies;
other member invocations are outside this factory witness. API headers, HTTP,
retry/transport and signer are controlled source-contract import bindings;
query inputs stop at ordinary ASCII scalar records. Native book helpers cover
ordinary dense arrays and numeric/raw string levels; actual source helpers are
used by the oracle. Eager detached snapshot spawn/rejection monitoring is a
mandatory host contract. Candle retry after cleanup and snapshot completion
after book cleanup retain the source behavior.

Not run: full Orderly/older test targets, workspace check/tests, host/wasm Clippy,
wasm builds, TypeScript/monorepo suites, CI reruns, browser/live exchange tests.
No shared SDK type/trait changed. Complete concrete class/member/HTTP/websocket/
signer/runtime assembly, arbitrary coercions/iterators/accessors/proxies/sparse
arrays and general event-loop scheduling remain outside this finite floor.
Canonical scope: `sdk-rs/parity/MANIFEST-SPEC.md#orderly-api-and-public-feed-bodies`.

## API and feed merge preservation

Fetched integration `d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e` merged down
without conflicts as `aefd1c4f7615f3d0235475de9c4ff037663acf71`. Standalone

```
git diff --exit-code 45dae1110151feb503e2e7b16fd4cc863123e99e HEAD -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests
```

returned **0**. Source and tests are byte-unchanged from the gate SHA. Additional
scoped diffs of controls/venue Cargo.toml/Orderly manifests, canonical notes,
compiler/formatter configs, extractor/Yarn/package inputs, source Orderly and
book utility TypeScript all return **0**. Tomllib traverses and compares all
**437** selected Orderly lock records: identical. Source/test/script/Cargo.toml
inputs of all **5** selected local crates also have scoped diff exit **0**.
Therefore Cargo/test/mutation/review/fmt gates stand and were not repeated.

Other venue manifests changed. Extractor check reran on the merge SHA: **0**,
7341 symbols / 512 files, would update 0, added 0, orphaned 0. Recursive tomllib
against fetched integration measures **7 pending -> ported, 0 -> divergent**:
**72 / 16 / 7 -> 79 / 16 / 0**, 95 total. This record changes tracked evidence
only; standalone source/test diff repeats after its commit and before push.
