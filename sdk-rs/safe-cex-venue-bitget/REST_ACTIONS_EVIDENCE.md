# Bitget REST amendment and account settings

Branch `feat/rust-bitget-rest-actions`, fresh from integration base
**500345147d6c0d4671f1b7382b1d83902fdcf3fb**. Body commit
**331488888b24de03136fe7cfe60d0c8cba40358b** has that immediate parent;
**32dd41cef6e781aeb5b42825fc3c2a059316aaf9** fixes only the source-oracle
HTTP getter syntax and has 331 as its immediate parent. No separate unreviewed
lane branch was used as an ancestor. Review and gates below identify their exact
SHA separately from subsequent evidence/integration commits.

## Rows moved and actual bodies

Python 3.11 **tomllib** compared `(manifest, kind, ts)` identities between
integration base and **32dd41cef6**: **9 pending -> ported, 0 -> divergent**.
Eleven Bitget manifests, 227 unchanged identities; **103 ported / 80 divergent /
44 pending -> 112 / 80 / 35**. Enumeration stopped at sorted file eleven,
`bitgetusdc.exchange.ts.toml`. This census is distinct from behavioral proof.

Actual executable targets in `src/rest_actions.rs`:

| Source | Rust body |
| --- | --- |
| BitgetExchange#tryAmendLimitOrder | try_amend_limit_order |
| BitgetExchange.updateOrder | update_order |
| BitgetExchange.updateAlgoOrder | update_algo_order |
| BitgetInverseExchange.updateAlgoOrder | inverse_update_algo_order |
| BitgetExchange.setLeverage | set_leverage |
| BitgetInverseExchange.setLeverage | inverse_set_leverage |
| BitgetExchange.changeMarginType | change_margin_type |
| BitgetInverseExchange.changeMarginType | inverse_change_margin_type |
| BitgetExchange.changePositionMode | change_position_mode |

The bodies retain source guards, presence (`in`) versus truthy/nullish tests,
callable capture and argument order, payload fields, branch-specific routing,
cache effects and catch boundaries. Amend distinguishes null fallback from an
empty result; construction/adjust errors precede its silent try/catch. Update
retains virtual before algo routing, exact amend-result return, legacy drag
fallback, explicit undefined trigger/update fields, and cancel before place.
Classic algo strips the virtual suffix, preserves sized stops and schedules on
truthy result; inverse retains the full ID, computes size and then clears it via
the source `!payload.newSize` check, and never schedules. That inverse behavior
is preserved, not corrected. Leverage retains side-first/nullish symbol fallback
before missing-market checks, min/max read order, unchanged guard, Classic
UTA+isolated+hedged v3 rules versus inverse Hedged-only holdSide, outside-try
inverse coin lookup, and string versus numeric leverage. Classic margin deletes
only the selected symbol from cache then inflight before fetch; inverse leaves
both alone. Position mode replaces both with distinct fresh objects before fetch.
Error emit argument counts, source catches and original error identity survive.

The 32 exemptions were generated from the actual future alias, venue-local host
trait, associated types and methods. Supplied native contracts bind receiver
paths, captured member owners, ordinary assignment versus own definition,
inherited property presence/reads/deletes, original array methods, operators,
intrinsics/imported functions, iterator destructuring/close, error constructors
and independently owned async work. No class/constructor/callee target was
fabricated. Concrete Exchange/HTTP assembly remains pending. No shared type,
trait, dependency or lockfile changed in this slice.

## Native evidence and actual-source floors

Six tests in `tests/rest_actions.rs` assert decoded native variants/fields,
independent counts, f64 bits, UTF-16 strings, original amend-array/order/position/
Error references, cache object identity and distinct fresh global objects before
any wire/source assertion. Payload objects are checked directly; expected fields
come from independent fixture recipes, not serialized bytes. Request endpoint selections,
argument counts, cancellation order, error reporting, numeric-versus-string
leverage, explicit undefined fields and retained inverse size behavior are native
assertions. Every native check for a test's group finishes before source comparison.

The oracle extracts and executes the nine actual TS initializers, actual enums,
endpoint constants, success helper, adjust and its actual math helpers/cache loop.
Receiver/HTTP/store/classification/margin-coin/UUID collaborators are fixtures.
Native adjust uses safe-cex-num; nonce is fixed, not RNG proof. Captured store bindings have native owner assertions; the actual-source oracle
checks HTTP/store `this` owners. Native HTTP-owner binding remains a supplied
contract, not a separate decoded-native identity assertion. The numeric fixture domain is
finite; exotic/proxy/prototype/getter/coercion and intrinsic binding domains are
not established by these tests. Serialization is a later diagnostic comparison,
not the native gate or a decode/re-encode witness.

Fixed **65-case** floor:

| Group | Independent floor | Stopped at |
| --- | --- | --- |
| Limit amend | 10 | index 9, schedule failure returns null |
| Update routes | 9 | index 8, explicit undefined replacement fields |
| Classic/inverse algo updates | 12 | index 11, inverse missing-market error |
| Classic/inverse leverage | 16 | index 15, inverse API code error |
| Classic/inverse margin | 12 | index 11, inverse fetch rejection |
| Global position mode | 6 | index 5, fetch rejection after cache replacement |

Coverage includes sparse order slots skipped by some/filter, virtual child guard,
invalid price, single-field amend, caught request versus uncaught adjust errors,
empty amend return, UTA/Classic/algo/virtual/legacy routes, current sized stop
fallback, inverse size overwrite, high clamp, unchanged leverage, side fallback,
hedged cross differences, per-symbol versus global cache state after fetch errors,
missing symbol/market and API errors. These floors do not enumerate all getter
changes, errors, numeric/array/status domains or timings.

## Sub-agent review citation

`/root/account_read_review` reviewed the complete committed slice at exact
**32dd41cef6e781aeb5b42825fc3c2a059316aaf9**, with integration base
**500345147d6c0d4671f1b7382b1d83902fdcf3fb** and immediate parent 331.
No blocking body or mapping findings across the nine rows within supplied-native
scope. Reviewer checked capture/read ordering, lazy position selection, source
inverse size behavior, presence-sensitive updates, catch/error identity, route
and cache distinctions, original aliases and native-before-source assertion order.

Reviewer measured git reads **0**; tomllib exactly nine pending -> ported and
32 unique actual helpers **0**; integration ancestor check **0**; committed-source
mode floor six with independent route/payload/error/cache/store-count assertions
node/Python **0**. Loader root rebased for node -e and five TS dependencies checked
byte-identical to reviewed SHA. Stopped at mode case index 5 (fetch rejection).
No edits. Reviewer did not run Cargo/native tests, extractor, mutation controls,
host/wasm clippy, CI or workspace checks. Review does not cover later integration
commits. The 65-case and declared native-binding scope is not an exhaustive
changing-getter/proxy/coercion/temporal ownership proof.

## Limits and checks not run

No workspace builds/checks, repo-wide suites, host clippy, wasm clippy, CI reruns,
live credentials/network tests or end-to-end Exchange execution. Concrete runtime
assembly, eager promise scheduling/ownership/cancellation, arbitrary native
callee overrides/proxies/prototypes/getters/coercions and full numeric/UTF-16/
crypto/RNG/array domains remain unproved. Native binding obligations are explicit
contracts; pending assembly/callee/class rows stay pending. Scoped crate gates
are not workspace compilation.

## Positive gates measured at 32dd41cef6

At exact committed **32dd41cef6e781aeb5b42825fc3c2a059316aaf9**:

| Gate | Actual exit and observed output |
| --- | --- |
| Scoped check | **0**, `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2`, Finished dev |
| New native/source tests | **0**, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test rest_actions --jobs 2`, six passed, 65 bounded source cases |
| Extractor checker | **0**, `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`; output read, zero would-update/additions/orphans |
| Independent tomllib census | **0**, nine pending -> ported / zero divergent, 11 files / 227 identical identities |

Tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo PATH prefixed `/opt/homebrew/opt/rustup/bin`, `--jobs 2`, one Cargo at a time.
Process completions were observed; logs did not use exit-masking pipelines.
The initial manifest writer ran after edits before body commit: **0**, output
read, zero updated/added/orphaned. A later writer overlapped deliberate expression
mutations and is a diagnostic only, not an unmutated-commit gate. Fresh post-merge
writer/checker measurements are recorded separately below.

Precommit check/test attempts exited **101** for result-type/semicolon and unused
fixture code compilation errors; the first compiled test run exited **101**
because the source-oracle HTTP descriptor lacked a closing brace. The latter is
corrected by 32dd's source-script-only follow-up. These failures are not mutation
gates or discovered production divergences. Positive native/source tests above
passed after those corrections. `node --check` for the repaired oracle exited **0**.

## Discriminating controls at 32dd41cef6

Each control separately changed only the stated production expression from exact
committed **32dd41cef6e781aeb5b42825fc3c2a059316aaf9**. Each compiled
(`Finished test`), exited **101**, and failed the following named decoded-native
assertion before source/wire comparison. Original bytes were restored after each.
No survivor or compilation-only failure was omitted.

| Control | Test selector | Native failure |
| --- | --- | --- |
| amend-size | `native_amend_then_source` | assertion `left == right` failed: REST native independent own field count amend payload |
| update-routing | `native_update_route_then_source` | assertion `left == right` failed: REST native UTF16 result.0 |
| classic-algo-id | `native_algo_updates_then_source` | assertion `left == right` failed: REST native UTF16 algo payload.orderId |
| inverse-algo-size | `native_algo_updates_then_source` | assertion `left == right` failed: REST native UTF16 algo payload.size |
| classic-leverage-category | `native_leverage_then_source` | assertion `left == right` failed: REST native UTF16 leverage payload.category |
| inverse-leverage-cross | `native_leverage_then_source` | assertion `left == right` failed: REST native independent own field count leverage payload |
| classic-margin-cache | `native_margin_then_source` | assertion `left == right` failed: REST native independent own field count per-symbol cache |
| inverse-margin-cache | `native_margin_then_source` | assertion `left == right` failed: REST native independent own field count per-symbol cache |
| global-mode-cache | `native_position_mode_then_source` | assertion `left == right` failed: REST native independent own field count fresh global empty cache |

After all nine controls, the complete `--test rest_actions` run exited **0**,
six passed. Subsequent scoped source/test/script restoration diff, edition-2024
rustfmt check on both new Rust files and patch whitespace check all exited **0**.
Controls used `--exact`, TS environment and Cargo jobs as stated above. Nine
controls are a bounded floor, stopping at index 8 (global mode cache contents),
not an exhaustive mutation enumeration.

## Full scoped crate suite

At exact committed **32dd41cef6e781aeb5b42825fc3c2a059316aaf9**, full Bitget
crate `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
--jobs 2 -- --include-ignored` exited **0**, with the TS environment above.
All emitted test-result summaries passed, including the six new REST tests;
last integration binary `tests/types.rs` passed four, then the doc-test floor was
zero (not a behavioral gate). Process completion was observed. This was the full
Bitget crate command, not a workspace/monorepo build or CI run. Source/test/script
byte restoration diff, edition-2024 formatting, patch whitespace and oracle
syntax were measured individually with subprocess return codes **0** during the
restored run. Those inputs match 32dd; only this untracked evidence file changed.

## Integration merge and preservation

Fetched and merged integration **0e014c0aa040874ed83ad9f13d9ae617c57a6435**
into the branch; exact merge SHA **bee572620b6ce9dc900f662d768496c0c81ed0e1**.
The sole conflict was parity/exempt.toml. Resolution used the complete stage-3
integration file plus the generated 32 actual Bitget records. Python tomllib
asserted every integration row retained in order, every own row retained, unique
Rust symbols and the independent helper count; exit **0**. No CLAUDE conflict.
The incoming integration work is outside this slice's review scope; no separate
unreviewed lane branch was used as a base or merged as an ancestor.

At exact merge SHA **bee572620b6ce9dc900f662d768496c0c81ed0e1**:

- Scoped `git diff --exit-code 32dd41cef6e781aeb5b42825fc3c2a059316aaf9
  HEAD -- <crate>/src <crate>/tests <crate>/scripts`: **0**, byte-identical.
- Wider recursive Cargo/dependency/build/lock/TS/package input diff: **1**,
  solely Cargo.lock adding existing base64/crypto dependencies to Blofin's record.
  This is disclosed, not called a byte-identical lockfile.
- The same diff excluding Cargo.lock: **0**, including local candle-cache,
  crypto, exchange, num, store, transport, types; workspace Cargo.toml/.cargo;
  venue Cargo.toml; all safe-cex and json-parse TS; package.json and yarn.lock.
- Independent old/new Cargo.lock tomllib comparison: **0**, identical record
  keys and lock version, only safe-cex-venue-blofin changed. Every one of **481**
  packages transitively reachable from safe-cex-venue-bitget has identical records
  and dependencies in both locks; Blofin is outside that graph. BFS enumeration
  stopped at index **480**, ucd-trie 0.1.7 (crates.io). This is dependency-record
  preservation, not workspace compilation or a host/wasm feature sweep.
- Manifest writer **0**, output read: zero updated/added/orphaned.
- Extractor checker **0**, output read: zero would-update/added/orphaned.
- Independent tomllib census against the merged integration head **0**, exactly
  **9 pending -> ported, 0 -> divergent**, 11 files / 227 same identities,
  103/80/44 -> 112/80/35; stopped at sorted file eleven named above.

Cargo checks/tests and mutation controls were **not rerun after merge**. Their
source, native tests, oracle scripts, Rust/TS/build inputs and reachable dependency
records are unchanged. Their exits remain measured at **32dd41cef6**, not fresh
measurements at the merge or subsequent evidence-only commit. The merged writer,
checker, census and preservation measurements above are tied to **bee572620b**.
Final pre-push fetch still observed integration **0e014c0aa0**. The following
commit changes only this tracked evidence; fresh scoped byte checks verify that.

## PR 7700 conflict repair: fresh merged-tree gates

The previous report did not imply the slice was merged upstream: PR 7700 remained
open/conflicting. This repair preserves its **nine pending -> ported, zero
-> divergent** rows; it does not count nine additional ports.

Merged integration **f6c865ddd8331c32516f063d90d045761bd2c73b** into the existing
branch, exact merge **b9df9078e53794e70e5b99e2c81060df2fcea93c**. The only
conflict was parity/exempt.toml. Complete stage-3 integration records plus the 32
actual REST helper records were retained, with tomllib uniqueness/full-record
assertions **0**. No CLAUDE conflict or unreconciled file remained.

At confirmed committed **b9df9078e53794e70e5b99e2c81060df2fcea93c**:
scoped all-target check **0**; full Bitget tests with ignored tests and TS environment
**0**; writer **0**, zero updated/added/orphaned; checker **0**, zero would-update/
added/orphaned; independent tomllib census **0**, nine pending -> ported / zero
divergent against f6, 11 files / 227 identities, 103/80/44 -> 112/80/35. Source/
test/script preservation versus reviewed 32dd and patch whitespace each measured
**0**. Initial check/writer invocations overlapped merge-commit bookkeeping and
also completed **0**; the cited confirmed runs above were started after verifying
committed HEAD b9. Cargo commands and logs were the same scoped commands above,
with measured subprocess return codes and HEAD consistency assertions.

`/root/account_read_review` reviewed exact committed **b9df9078e53794e70e5b99e2c81060df2fcea93c**.
Parents were verified as 82ed19a4d1 and f6c865ddd8. Reviewer measured committed
source/test/script byte preservation **0** versus reviewed 32dd; complete integration
exemption records plus exactly 32 own records/unique targets **0**; exact nine mappings
and unrelated-row/file-metadata preservation **0**; no unrelated own changes **0**.
No blocker. A reviewer diagnostic exited **1** from assuming mirror mappings must
append to rust_files; both inventories were unchanged and the corrected preservation
check passed **0**, not an artifact defect. No reviewer Cargo, runtime/source tests,
extractor, controls, clippy/wasm/workspace/CI at b9; the earlier reviewer mode-six
runtime measurement remains attached to 32dd. No edits.

Integration advanced while the full suite ran. Merged latest observed integration
**c394ef75eec20388a2dbb80d9d69c0bf0fa69a4c**, exact merge
**dbec9d98d5756674230f00a76009fa34e5f24421**. No conflict. Bitget src/tests/scripts
versus b9 are byte-identical (scoped diff **0**). New Poseidon modules changed the
crypto dev dependency; the lockfile also changed. Rather than claiming unchanged
build inputs, fresh Cargo gates were measured again at committed dbec:

| Fresh gate at dbec9d98d5756674230f00a76009fa34e5f24421 | Actual exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, Finished dev |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` with TS environment | **0**, all emitted summaries passed, last types binary four passed, doc-test floor zero |
| Manifest writer | **0**, output read: zero updated/added/orphaned |
| Extractor checker | **0**, output read: zero would-update/added/orphaned |
| Independent tomllib census versus c394ef75ee | **0**, nine pending -> ported / zero divergent, 11 files / 227 identities, 103/80/44 -> 112/80/35 |
| Complete latest integration exemptions plus 32 exact own records / unique targets | **0** |

Cargo remained sequential with two jobs; actual completion was observed and HEAD
consistency checked for each fresh run. Census enumeration stopped at sorted file
11, bitgetusdc.exchange.ts.toml. Reviewed source remains unchanged; the b9 review
does not cover unrelated incoming crypto/integration code. The nine compiling
mutation controls were **not rerun** in this repair: their actual exits remain at
32dd, with own src/tests/scripts byte preservation proved. Workspace checks,
host/wasm clippy, CI and live tests remain unrun. Concrete assembly remains pending.

Final pre-push fetch observed integration **3cc94529835dde67ba3e6af513a233baa307c3f0**;
merged cleanly, exact merge **a86c040b609bbec7ce48f020c6a23d9a2876084e**.
At a86, own src/tests/scripts and recursive Rust/dependency/build/TS/package inputs
excluding Cargo.lock each diff **0** versus fresh Cargo gate SHA dbec. Cargo.lock
changed only the unrelated safe-cex-venue-weex record; tomllib key/record comparison
and traversal proved all **481** Bitget-reachable records/dependencies identical,
Weex unreachable, exit **0**. BFS stopped at index 480, ucd-trie 0.1.7 (crates.io).
This is not a byte-identical whole-lock claim. Cargo tests/checks were not repeated
a third time after this metadata-only merge; their actual fresh exits remain tied
to **dbec9d98d5**. Source, build, test and reachable dependency inputs are preserved.
Fresh writer and checker at **a86c040b60** each exited **0**, output read: zero
updates/additions/orphans. Tomllib census versus latest merged integration exited
**0**, preserving exactly nine pending -> ported / zero divergent, 11 files / 227
identities, 103/80/44 -> 112/80/35; stopped at sorted file eleven as above. This
following evidence-only commit does not introduce another code/gate SHA. Final
scoped byte-preservation checks are measured before push. No next-slice code was
started during the PR repair.
