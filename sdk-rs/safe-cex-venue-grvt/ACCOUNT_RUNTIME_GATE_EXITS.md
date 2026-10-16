# GRVT account runtime gate exits

Fresh branch: `feat/rust-grvt-account-runtime`. Actual integration base:
`e0e1ceb8da277e79fa4bf29cbef4e94fd1dba0f0`. No unreviewed feature branch is
an ancestor before this slice. No shared type/trait was changed.

## Rows moved

Independent Python 3.14 tomllib census, matched TS symbol names to the base:
164 GRVT rows. Base 96 ported / 48 divergent / 20 pending; slice 106 ported /
48 divergent / 10 pending. **10 pending -> ported, 0 -> divergent**.
Targets: tick, authenticate, validateAccount, fetchBuilderAuthorization,
getBuilderInfo, start, fetchBalance, fetchPositions, fetchOpenOrders, mapOrder.
These are source bodies over AccountHost/SessionHost operations. The class and
constructor rows, concrete client/Store/socket adapters, placeOrder,
fetchFillsForOrderIds, and the two signing helpers remain pending. Census is
not proof of assembled venue runtime completeness.

## Commands and exact revisions

Cargo binary `/opt/homebrew/opt/rustup/bin/cargo`; every invocation serialized,
`--manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --jobs 2`.
Source oracles used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Node 24.20.0 was default; Node 26.7.0 used PATH prefix
`/opt/homebrew/opt/node@26/bin`.

At `48f14f1920c0f88a8b46bff776dbab87e606d221`, first account_runtime test
command exited 101 during compilation: missing parseInt radix argument and a
borrowed temporary in the test host. No assertion gate passed at that revision.
At `b660622549de5a76eada27cfe9487ee7a0471318`, the scoped test command exited
101: 6 passed, 4 failed. Instrument problems were enum spellings, the omitted
native tick binding, an accidental full metadata field on the lite oracle
fixture, and an oracle getter that appended again during result inspection.
Corrected the test host/fixtures; production bodies were unchanged.

At `cfdb5b5113ad8798a2da43d64da551fbe3cb3a4f`:

- `cargo test --test account_runtime` with common arguments: exit 0 on Node 24
  and separately Node 26, 10 tests in each run.
- `cargo check --all-targets` with common arguments: exit 0, zero errors.
- Pinned `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  exit 0; inspected output: 7341 symbols / 512 files, would update 0 manifests,
  0 added, 0 orphaned.
- Ten row controls below: each compiled broken mutation produced a named native
  assertion failure, exit 101; byte-exact restoration and the same exact test
  exited 0. The first mapOrder control replaced book with amount and hit an
  unused-book compiler error (101), so it was NOT counted as discrimination.
  Revised that control to double book, measured the named assertion failure
  and restoration. No executable mutation survived.

At `53ada22ced12e2e186921aaa1e0d1235584f15c6`:

- `cargo test --test account_runtime`, Node 24: exit 0, 10 tests.
- Same scoped target, Node 26: exit 0, 10 tests.
- `cargo check --all-targets`: exit 0, zero errors.
- Pinned extractor check: exit 0; 7341/512, would update 0 manifests,
  0 added/orphaned. Output inspected, not inferred from the Yarn shim.
- Two additional discrimination controls for review findings below: 101/0 each.
- Working source/tests diff exit 0 after all restorations; `git diff --check` 0.
- Source comparison cfdb -> 53ad: exit 0. Test changes added live callback,
  detached-task and alias witnesses; the ten row controls remain explicitly
  attached to cfdb, the extra controls and successful final gates to 53ad.

The writer ran after manifest edits and before the implementation commit:
updated exactly one manifest, 0 added, 0 orphaned; writer-managed file arrays
were committed. Generated captures/logs were kept outside Git.

## Discrimination

Each exact command added `--test account_runtime <test> -- --exact` to the common
cargo arguments. All valid compiled controls failed 101, then restored passed 0.
No wire or re-encoding comparison precedes native assertions.

| SHA | Test | Mutation | Named failure |
| --- | --- | --- | --- |
| cfdb | tick_source | interval 60000 -> 60001 | tick/success effects native number |
| cfdb | authenticate_source | cookie TTL 3600 -> 3601 | authenticate/success cookie native number |
| cfdb | validate_source | successful error string changed | validate/success result native string |
| cfdb | builders_source | only two retry attempts | builders/retry effects native array shape |
| cfdb | builder_info_source | required fee 0.03 -> 0.04 | builderInfo/unauthorized error native number |
| cfdb | start_source | omit orders loaded flag in public mode | start/public effects native bool |
| cfdb | balance_source | used equity-minus-available -> plus | balance/lite result native number |
| cfdb | positions_source | signed contracts instead of abs | positions/lite result native number |
| cfdb | orders_source | instrument_id -> instrument_ids | orders/symbol effects native key instrument_ids |
| cfdb | map_order_source | double book remaining | mapOrder/full result native number |
| 53ad | tick_source | retained callback becomes no-op | tick/loop-fire effects native array shape |
| 53ad | positions_source | clone direct fallback array via map | positions/no-account native fake array identity |

## Review and enumeration floor

Production sub-agent review approved the ten bodies. Test sub-agent review found
that the disposed loop-fire fixture could pass without callback dispatch and that
fake-position structures did not establish alias identity. Both findings were
corrected at 53ad and independently discriminated there. Reviewers ran no gates.

Actual TS member text is extracted by AST without rewriting bodies. Only selected
members are assembled into the oracle class; constructor/class completeness is
not tested or claimed. Bound source helpers use their actual declarations.
Independent expected group counts: 11 / 10 / 8 / 18 / 7 / 18 / 14 / 19 / 10 / 14
= **129 cases**. Floors stop at tick/loop-fire, authenticate/null-error,
validate/emit-error, builders/iterator-error, builderInfo/fetch-error,
start/last-log-error, balance/prototype-currency, positions/object-symbol,
orders/map-append, mapOrder/status-mutation. Builder retries stop at the source's
third attempt; no unbounded enumeration or exhaustive interleaving claim.

Native assertions cover decoded values, own undefined fields and nonfinite
numbers, eager state/effects and final effects, method receivers, raw exception
identity, retained loop callbacks, input info identity, direct fake-array identity,
fake element/duplicate aliases and object-symbol identity. The startup final-log
failure fixture executes the actual tick body and observes its independent
continuation after startup rejects. Live array append and custom iterator cleanup,
Store.update getter mutation before loaded spread, SDK callee capture before
endpoint await, and return-promise adoption outside local catch have witnesses.

Concrete adapters, arbitrary getter/coercion combinations, native prototype
function identity, sparse arrays and all promise/iterator interleavings are not
proved. No NaN payload or signed-zero bit claim. Source-prescribed catch/logging
behavior is kept. Existing venue tests other than this target were compiled by
all-target check but not executed in this slice.

Not run: workspace check, full crate/monorepo suites, host clippy, wasm check,
wasm clippy, CI, live exchange calls, browser tests, concrete adapters.

## Integration preservation

Merged integration `1674d852f7687b8d427cd4b77c3aee067688979f`, producing
`8965ee65dbc2b30e0905c5abc90cfa33e681279b`. At that merged revision:

- `git diff --exit-code 53ada22ced12e2e186921aaa1e0d1235584f15c6 HEAD --
  sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests`: exit 0.
- Same comparison for workspace/venue Cargo.toml, complete exchange/types/num/
  store/crypto/transport dependency crates: exit 0. Cargo.lock unchanged.
- Same comparison for GRVT TS files, types.ts, createPositionId.ts, LogSeverity.ts
  and GRVT manifests: exit 0.
- Pinned extractor check: exit 0; 7341/512, would update 0 manifests,
  0 added/orphaned. Output inspected. Successful scoped gates at 53ad stand.
- Repeated tomllib count: 106/48/10, ten pending -> ported, zero -> divergent.
- `git diff --check`: exit 0. Tracked tree clean; only the user's three untracked
  .aimux instruction files remain.

Pre-push fetch advanced integration to `5d104b35e2487e910db00b1770cad99228c94341`.
Merged result `824076a704761ab5cea0428e1812a768fb71a59b`. Repeated all source/tests,
Cargo/dependency/lockfile and oracle/manifest comparisons against 53ad: exit 0
for each; `git diff --check` 0. Extractor check at 8240: exit 0, 7341/512,
would update 0 manifests, 0 added/orphaned; output inspected. Scoped gates stand.
