# WOO legacy order control evidence

Fresh branch `feat/rust-woo-legacy-order-reads` starts at integration
`444310f59d21ba618311abde370f93573450538a`. No additional unreviewed feature
ancestor was used. Code commit: `46460a8fb5bea3dfa555ad3f487e43336aa9f65a`.
Integration `372972fbfaa126366d6309f4ba15fa84d07bc7a5` was merged down as
`423ce8793a852548a218d2c928fb1388fe98f4fb`. This evidence is authored manually;
generated logs, module records and mutation scripts remain in `/tmp`.

## Scope and census

Eight pending rows moved to **ported**, zero to divergent: `fetchOrders`,
`fetchLimitOrders`, `fetchAlgoOrders`, `cancelOrders`, `cancelAlgoOrder`,
`getPositionSideForOrder`, `cancelAllOrders`, `cancelSymbolOrders` in
`woo.exchange.ts`. Native method bodies live in `legacy_order_control.rs` over
compatible JS-value, captured-callback, imported-helper and transport primitives.
Class/runtime wiring remains a separately pending row. No v3-owned file was
edited by this slice.

Python 3.11 `tomllib` independently enumerated the three owned legacy manifests
and asserted the fixed 101-row expectation, row identity and exactly eight
pending-to-ported transitions. Parent: **38 ported / 20 divergent / 43 pending**.
Code and merge: **46 / 20 / 35**. Census iteration stopped at row 100; this is an
ownership census, not a proof of the whole venue's runtime.

## Measured gates

All Cargo invocations are scoped to `safe-cex-venue-woo`, jobs 2, one at a time.
Oracle invocations use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

At code `46460a8fb5bea3dfa555ad3f487e43336aa9f65a`:

| Command | Exit | Observed result |
| --- | --- | --- |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_order_control --jobs 2` | 0 | 9 passed, 0 failed; 40 ordinary cases plus three callback/timing witnesses |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

Before the code commit, the extractor writer ran on identical manifest/source
inputs: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`,
exit 0, Updated 0 manifests; 0 added, 0 orphaned symbols. No `rust_files` array
was hand-written.

At merge `423ce8793a852548a218d2c928fb1388fe98f4fb`:

| Command | Exit | Observed result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

The nine-test Cargo target was not rerun at the merge: all reachable inputs
were verified unchanged, as detailed below. These tests and mutation exits
remain measured at the code SHA; they are not relabeled as merge executions.

Development instrumentation initially failed compilation with exit 101 because
an unused test helper tripped `-D warnings`; it was removed. This compiler-only
failure is not credited as a discriminating mutation. An earlier 8-test target
passed before the additional capture witnesses; only the strengthened 9-test
result above is the code gate.

## Independent corpus and native assertions

The source oracle extracts the eight actual TypeScript AST members, actual
WOO constants and enums, actual `reverseSymbol` and `omitUndefined`, and the
installed `p-iteration.forEachSeries`. It does not substitute copied method
bodies. Independent Rust fixtures assert native outcome, original error and row
identity, callback order, counts, option placement, typed fields, numeric parsed
IDs including NaN, UTF16 URL code units, undefined versus null, and sibling
recreation fields before any wire comparison in each test. Successful mappings
retain the input row references; comparisons are not decode/re-encode roundtrips.

Fixed ordinary corpus expectations: combined 3 (stopped 2), limit 5 (4), algo 5
(4), batch 8 (7), single 7 (6), side 6 (5), all 3 (2), symbol 3 (2): **40** total,
stopped global row 39. Rust pins each independent family and local index as well
as each fixture's input and result. A separate **3**-case capture corpus pins
place, update, await order, stopped witness 2. Its native assertions check
captured generation and keys after mutable getters, updated ID after capturing
the status callback, and eager first element PromiseResolve with deferred DELETE.
This is a 43-case floor, not exhaustive JavaScript semantics or network coverage.

## Runtime mutation proof

All controls below are bound to code SHA
`46460a8fb5bea3dfa555ad3f487e43336aa9f65a`. Source and oracle were restored
byte-identically after each batch/control. Each credited failure is a runtime
panic; no compile failure is counted.

Eight independent method mutations were batched into one scoped Cargo run with
`-- --skip captured_callbacks_native`: exit **101**, **0 passed / 8 failed**.
Each named family test failed at its native assertion on local iteration 0:

| Mutation | Test and first named assertion |
| --- | --- |
| Call limit reader again instead of algo reader | `combined_native`: `ORDER native combined sequence iteration 0` |
| Select algo mapping for limit wrapper | `limit_native`: `ORDER native mapped orders size 1` |
| Select limit mapping for algo wrapper | `algo_native`: `ORDER native mapped orders size 4` |
| Admit duplicate truthy parent ID | `batch_native`: `ORDER native batch sequence iteration 0` |
| Suppress sibling recreation | `single_native`: `ORDER native single sequence iteration 0` |
| Reverse hedged LONG/SHORT selection | `side_native`: `ORDER native position side` |
| Cancel-all uses symbol route | `all_native`: `ORDER native cancel all route` |
| Symbol cancellation places arguments in data | `symbol_native`: `ORDER native symbol options` |

Restore, same scoped Cargo command: **0**, **8 passed / 0 failed**, one filtered.
Then move `Place` capture before both sibling type getters; filtered scoped Cargo
run of `captured_callbacks_native -- --exact`: **101** at
`ORDER native place capture generation`. Restore: **0**, one passed, eight
filtered. All three capture witnesses ran on restoration.

Two oracle-output controls used the restored Cargo-built
`legacy_order_control-6f18293ad3741feb` binary, filtered to `combined_native`:

- Return only 39 of the 40 rows: **101**, `ORDER corpus SIZE`; restore **0**,
  one passed, eight filtered.
- Return 40 copies of the first row: **101**,
  `ORDER corpus CONTENT index iteration 1`; restore **0**, one passed, eight
  filtered. This corpus has the correct size and is rejected for content.

Distinct credited control floor: **11**, stopped control 10. None of those
runtime controls survived. Review initially found that prepared callback tokens
were ignored by the mock, leaving capture ordering unproved. The mock now checks
all operation tokens and the three actual-source witnesses close that reviewed
instrument gap. No unexecuted capture mutation is claimed as a gate.

Restored SHA256s:

- `src/legacy_order_control.rs`:
  `db2df051290408e86580a825aa335d278a165761d710ff50a8d3ed107c4102aa`
- `tests/legacy_order_control.rs`:
  `7de1f6314aeb42a1f48b8608e2db3dc74b92bd459f794666b2d4343db9b02920`
- `tests/legacy_order_control_oracle.cjs`:
  `38c01525454d4737239a2d1b4923dbdb47e48bf91221af2ea2a171e8e0b96e29`

## Merge preservation and review

Measured `git diff --exit-code` between code and merge was **0** over the root
Cargo manifest, all eight reachable internal package source/test/manifests,
WOO manifests, the actual TS oracle inputs (exchange, constants, utils, enums,
get-key, omitUndefined and logging enum), and reachable transport frame input.
Reachable Cargo.lock package records compared independently equal: **481**,
stopped record 480. Internal packages were candle-cache, crypto, exchange, num,
store, transport, types and venue-woo. This does not assert unrelated lock
records or unrelated venue sources were unchanged.

An actual-source Node loader measured **132** externally loaded module files,
stopped file 131. The paths and SHA256 contents re-resolved identically after
merge. Native toolchain, OS, Node engine and complete module universe equality
are not claimed. The extractor was rerun because integration brought other
venues' manifest changes, despite unchanged WOO test inputs.

Read-only sub-agent review compared actual source and found/fixed initial
first-DELETE timing, eager then-getter timing, UTF16 URL construction, callback
capture order and strict versus Set comparison semantics. It reviewed the
strengthened native/source witnesses and found no remaining material defect in
that finite floor. The reviewer ran source timing witnesses with exit 0; it ran
no Cargo gate. Review is not a substitute for the measured gates above.

Not run: workspace compilation, full crate test suite, host or wasm clippy,
credentialed HTTP, live sockets, full adapter integration, v3 tests or CI reruns.
Standard array methods are modeled; overridden methods/species and complete
prototype/callback replacement coverage are not claimed.


## Shared strict-row decision and reduceOnly presence (2026-10-18)

The earlier eight-row operation-body proof above remains scoped to those raw
bodies. The configured production fetchOrders boundary now enforces shared-type
decisions 3/3b: omit only malformed amount/filled/reduceOnly rows, emit their
field, original typed value and id, and preserve valid neighboring rows. TS keeps
malformed rows; this refusal is an explicitly accepted divergence. Missing or
own-undefined reduceOnly is valid through ModelField<bool>, alongside false/true.
Public placement input remains strict, without bool/number/string coercion.

`mixed_algo_rows_refuse_only_the_bad_strict_field_through_the_real_binding`
executes each bad field through both the public Exchange read and the configured
fetch-to-Store path. One good limit row plus three good algo rows survive; the
undefined and false properties remain distinct. Against e6597a197f, the compiled
first case failed the whole read with invalid string amount. With the cutover,
all six paths pass and each reports exactly one refusal. The raw runtime-value
mapAlgoOrder/fetchAlgoOrders witnesses and fixtures are unchanged.

`safe-cex-exchange/tests/order_reduce_only_presence.rs` executes 48 actual-TS
Store combinations for single/bulk upsert and patch, observes Order/Prepared
native presence before JSON omission, and rejects non-booleans at output and
public input boundaries. The compiled baseline refused absent reduceOnly.
BingX binding's undefined amendment regression also compiled/fails when the
pre-cutover production sources are restored, then passes after exact restoration:
its entire wire body and HMAC match the actual maintained class and API signer,
with controlled clock/UUID entropy and installed qs/real HMAC.

Native scoped verification after merging master: WOO23 unit +37 order +18 read
+24 stream tests pass; BingX46 binding +10 completion pass; Apex5 unmapped/strict-row
tests pass; shared native-model8, Store contract/risk6 and Store composition1 pass.
Required all-feature cancellation7, override8 and binding25 checks pass. Type model
utilities2 pass with the existing live-source oracle still dependency-gated.
The extractor check reports zero updates. Compiler-only checks of all explicit
model consumers and their test targets passed before master integration; the
newly merged Apex false assertion was migrated and its real binding suite passes.


Final follow-up: the live type-model oracle also passes (3/3 with include-ignored).
Actual accepted native decoder probes pass for three model-record owners and five
ModelField/Prepared owners; the full types inventory records151 Deserialize and70
alternate-only owners. Native FFI/registry compiles. Strict Clippy passes for all21
affected production libraries on native and Wasm, plus WOO/BingX all targets on
both targets. Full parity passes7459 rows/16622 public items; extractor check0.
Only the five reviewed unchanged nested helpers' containing-file pins were
refreshed; no unrelated core cancellation/override body pin was changed.

After the final normal master merge (5a784f2f55), the required inventories pass
again (7 cancellation, 8 override, 25 binding). BingX binding/completion passes
58 tests; Apex binding/unmapped passes53. Native FFI/registry compile, full parity
(7459 rows/16619 public items), and extraction freshness (0 changes) pass.
