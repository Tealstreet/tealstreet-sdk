# Bitrue refresh entrypoints

**3 rows pending → ported; 0 pending → divergent**:
`BitrueExchange#handleFillForFastRefresh`,
`BitruePrivateWebsocket.handleAccountUpdate`, and
`BitrueExchange.changeMarginType`.
The independently parsed manifest census is **21 ported / 25 divergent / 61
pending**, against integration's 18 / 25 / 64. See `census.json` and
`merge-census.json`. The census floor ends after six manifests / 107 rows,
at `bitrue.ws-public.ts.toml` / `BitruePublicWebsocket.listenOrderBook`.

## Commit boundaries

- Fresh branch `feat/bitrue-refresh-entrypoints` starts from reviewed integration
  `d8c841d4c02209793adfbd58522f3db1316488ce`.
- Initial implementation `45f3d82056db724775dead3b9acb37ad2809d39e` was corrected
  after sub-agent review. Its measurements are retained in the adjacent
  [superseded evidence directory](../refresh-entrypoints-45f3d82056/README.md).
- Corrected implementation and control base:
  **`ecce21189f234cd2d6727e7b11c0f855b8ecf331`**.
- Final merge **`ab11e252c5e5501c93d942d00fb204ef896adfd4`** merges reviewed
  integration `cc3d25150b320656935009943f72d17b27e380a0`.
- `merge-comparison.json` records an exact `git diff --exit-code` result of 0
  for this crate's source, tests, scripts and Cargo file, the actual source
  venue, and the listed shared dependency source/Cargo/configuration paths
  between the corrected gate SHA and final merge. It does not claim every
  possible environmental input was compared. Gate exits retain their measured
  SHAs; no old result is relabeled as a new run.
- No separate unreviewed feature branch is an ancestor. The final evidence-only
  commit adds these records without changing code, tests, manifests or build
  configuration.

## Function parity and scope

Fill refresh resolves the callee before evaluating `fill?.symbol`, skips the
property read for nullish fills, and forwards exactly one argument. Its generic
symbol value retains UTF-16 strings, number bits, arbitrary native identity,
null and undefined without a JSON-only production model. Getter, resolver and
queue exceptions retain their original native value. The callback return is
ignored and success returns native void.

Account refresh never reads its payload, resolves the parent queue, invokes it
with **zero arguments**, ignores its result and retains failures unchanged.

Margin-type rejection ignores both arguments. Invocation eagerly emits the
exact source message, then constructs the error only if emission succeeds.
The generic constructor boundary supplies the complete native error value;
the function propagates that same value, including constructor failures,
without rebuilding an error from selected fields. The returned ready future
preserves effects before polling. Class construction, dispatchers, refresh
scheduling and host/dependency implementations remain pending. These are
three complete entrypoint body claims at explicit source dependency boundaries.

The final implementation declares no extra public enum. The superseded
implementation's helper enum and shared `ExchangeError` reconstruction were
removed, resolving the static mapping and host Error ABI concerns rather than
adding an exemption or weakening a gate.

## Native witness and controls

`refresh_entrypoints_match_actual_typescript` extracts and executes the complete
unchanged actual TypeScript arrow initializers. Dependencies capture receiver
binding, callee/property access, queue arguments, emission and error construction.
The instrumented Error constructor returns a real stock Error; the source
asserts its stack/prototype and tests identity before transporting observations.
Rust checks opaque native identity with `Rc::ptr_eq`, decoded native arguments,
ordered effects, and the before-poll effects of margin invocation. No Rust
output is serialized for comparison. All native assertions precede the
independent input inventory checks.

The fixed expectation is **16 fill / 6 account / 5 margin = 27 native cases**.
This case floor stops at margin case4 `constructor-throw`; it is not exhaustive
runtime enumeration. It includes nullish/missing arguments, an empty string,
unpaired surrogates, negative zero, NaN, false, native object identity, ignored
payloads, throwing getters/resolvers/queues/emitters/constructors, and ignored
dependency return values.

The following four controls were measured at **ecce21189f** plus only each
recorded mutation. Every mutant compiled and failed at the named native runtime
assertion; each exact source restoration then passed all 27 cases.

| Control | Named native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| Drop forwarded fill symbol | `BITRUE-REFRESH-NATIVE-EFFECTS fill case3 empty` | 101 | 0 |
| Skip account queue invocation | `BITRUE-REFRESH-NATIVE-EFFECTS account case0 undefined` | 101 | 0 |
| Alter margin rejection message | `BITRUE-REFRESH-NATIVE-BEFORE-POLL margin case0 false` | 101 | 0 |
| Regress margin to lazy async body | `BITRUE-REFRESH-NATIVE-BEFORE-POLL margin case0 false` | 101 | 0 |

There are no survivors among these four controls. This fixed control floor
stops at the fourth control, lazy invocation; no exhaustive mutation claim is
made. Patches, exact commands, variants and raw failures/passes are recorded
in the mutation JSON files, `gates.json` and individual logs.

The initial witness did not assert invocation timing or complete Error value
retention. Sub-agent review identified those gaps. The correction adds native
pre-poll and identity observations, and the explicit lazy-invocation control
proves the added timing assertion discriminates. Superseded passing tests do
not prove the corrected behavior.

## Not run

Full Bitrue suites, other crate suites, workspace compilation/suites, canonical
parity index validation, wasm runtime tests, live exchange tests, prior slices'
controls and CI were not run. Host and wasm clippy are separate compilation/
lint measurements, not wasm execution proof. Mutation controls, host/wasm
clippy and formatting were not rerun after the byte-unchanged final merge;
their recorded exits remain bound to ecce21189f. Final-merge native, scoped
check and extractor measurements are separately recorded.

## Measured gate ledger

Exact commands, elapsed times, SHA and dirty variant are in the JSON ledgers.
All Cargo invocations were sequential, scoped to `safe-cex-venue-bitrue`, with
`--jobs 2`. Source witnesses used the supplied TypeScript module path.

### Corrected implementation — `ecce21189f234cd2d6727e7b11c0f855b8ecf331`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| fill-mutation | 101 | [fill-mutation.log](fill-mutation.log) |
| fill-restored | 0 | [fill-restored.log](fill-restored.log) |
| account-mutation | 101 | [account-mutation.log](account-mutation.log) |
| account-restored | 0 | [account-restored.log](account-restored.log) |
| margin-mutation | 101 | [margin-mutation.log](margin-mutation.log) |
| margin-restored | 0 | [margin-restored.log](margin-restored.log) |
| lazy-invocation-mutation | 101 | [lazy-invocation-mutation.log](lazy-invocation-mutation.log) |
| lazy-invocation-restored | 0 | [lazy-invocation-restored.log](lazy-invocation-restored.log) |
| check | 0 | [check.log](check.log) |
| host-clippy | 0 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 0 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| writer | 0 | [writer.log](writer.log) |
| extractor | 0 | [extractor.log](extractor.log) |

### Final merge — `ab11e252c5e5501c93d942d00fb204ef896adfd4`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native | 0 | [merge-native.log](merge-native.log) |
| check | 0 | [merge-check.log](merge-check.log) |
| writer | 0 | [merge-writer.log](merge-writer.log) |
| extractor | 0 | [merge-extractor.log](merge-extractor.log) |

Both extractor writer outputs report Updated 0 manifests; both extractor
checks report Would update 0 manifests. Output texts were verified in addition
to measured process exits.

## Sub-agent review

`review_auth_witness` found no blockers in exact corrected commit
`ecce21189f234cd2d6727e7b11c0f855b8ecf331`. The reviewer confirmed all three
ported/adapted claims at their declared dependency boundaries, resolution of
the original invocation-timing/Error-ABI/mapping findings, and all four exact-SHA
patch/log pairs with named runtime 101 failures and restored 0 passes. It
confirmed the 27-case floor ending at margin case4 constructor-throw and no
survivors among the four controls. Its commit-to-commit comparison confirmed
that merge `ab11e252c5` preserves reviewed source, tests, oracle, manifests and
actual TypeScript entrypoint files. The reviewer ran no Cargo and made no
post-merge mutation/clippy rerun claim.
