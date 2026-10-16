# WOO V3 private regular/algo orders and transactions

Fresh branch feat/woo-v3-private-order-models from **0e116b60e90e37e08f3995645493fc20176c0b86**.
Measured source/tests/scripts/manifest SHA: **a9048b651d2d19eff7a2276cc83ac379f01fed21**, whose immediate parent is
**4265451f8f8b77cde4ae3fc048e1c97fb995e965** (the exhaustive output-check fix). The measured commit
removes its redundant trait import. Initial source feature d23a107647dfaefb3721b088d98eae899b1d08fe
descends directly from the fresh base; all three commits are this slice only.
No unrelated unreviewed branch is an ancestor. Merge-down parent **f6c865ddd8331c32516f063d90d045761bd2c73b**
at **5dd12405a62d525b9e8a71b7e90418227b61d67d**; scoped crate source/test/script/private-manifest comparison exited 0.
Only Cargo.lock changed among checked inputs: Bitget/Blofin dependency entries. An independent parent/merge comparison stopped at 481 WOO-rooted package records; all full records, identities and dependency edges matched. The list and receipt are in the JSON. Original source gate exits stand for this unchanged slice. Post-merge extractor
and census each exited 0 at the separate merge SHA, with the same 28 moves versus the actual integration
parent. This sheet/JSON are added by a later documentation-only commit; no source gate is claimed as
executed at that documentation SHA.

## Completed gates measured at a9048b651d2d19eff7a2276cc83ac379f01fed21

| Gate | Exit and observation |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same scoped package/all-targets/jobs/offline/locked settings | 0; 42 targets / 121 passes / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 files; Would update 0, added 0, orphaned 0 |
| python3.14 scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows / 405 public Rust items |
| rustfmt --check --edition 2024 src/v3_private_orders.rs tests/private_orders.rs | 0 |
| node scripts/private-orders-oracle.cjs | 0; actual whole private/types/safe-math/math/logging modules, pinned AST declarations |
| python3.14 scripts/private-orders-census.py 0e116b60e9 | 0; 28 explicitly expected pending-symbol moves |
| python3.14 scripts/private-orders-controls.py | 0; six compiling named native failures and six restored passes |
| git diff --exit-code after controls | 0; exact tracked restoration |

Cargo used /opt/homebrew/opt/rustup/bin/cargo, one invocation at a time with jobs 2. Oracle/tests/controls
used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Extractor output
text was inspected; parity:extract wrote the manifest, with rust_files derived from actual row targets.
Target indexing scopes 22 WOO/journal manifests and dependency symlinks, not workspace compilation.
The JSON retains SHA receipts, full commands, patches/call sites and named failure/restored summaries.

## Native fields and independent finite floors

Seven tests inspect each field of regular orders (24), recursive algo orders (35), transactions (13),
mapped transactions (5), deferred algo records (2) and algo mapping containers (2). Every required,
optional, nullable and recursive field is inspected directly. AST checks pin names, types, requiredness,
readonly arrays and every member of the seven gateway string unions. All 37 enum values are decoded
and compared to explicit native variants (floors 2/5/12/9/3/4/2); six invalid kinds/literals per enum are
refused. Decimal-vs-number quantities retain native variant and bits, including -0/infinity through
recursive child arrays. Transaction maker literal decode preserves -0 while enforcing 0/1; required
nullable PnL absence names the missing field explicitly. Native assertions run before any wire claim;
no Rust decode/re-encode equality or numeric Value intermediary is used for input/model comparisons.
There is no separate wire-equivalence claim.

Independent oracle walks stop at helper 22, regular decode 19, quantity 11, recursive algo decode 16,
explicit depth 2, transaction decode 11, page wrapper 18 (three named routes x six cases), regular mapping
12, algo mapping 13, transaction mapping 9 and declaration 10. Recursion checks include an accepted
eight-edge chain, a refused nine-edge chain, and depth 9 refusal before invalid input shape. These are
finite floors, not exhaustive JavaScript inputs, recursion/branch enumeration or live runtime proof.

The declaration floor contains an explicit 100/010/001 basis for isTriggered/reduceOnly/isActivated;
all three bool fields are independently inspected. Initial draft flags had reduceOnly=isActivated=false,
so static review identified a potential symmetric-swap survivor before runtime gates. The basis and the
actual compiling symmetric-swap control below close that specific gap. No measured survivor was dropped.

Regular decode checks amount, optional realizedPnl and optional marginMode before symbol/required
fields. Algo decode checks depth then optional type/margin then complete children before parent IDs.
Transaction decode checks maker then nullable PnL before IDs. Page wrappers retain the previously ported
metadata-before-row/cap ordering with exact names/paths. Regular mapping validates side/type/amount/
filled/remaining before status/symbol/price, retaining safe-math subtraction instead of ordinary rounding.
Algo visitation validates side/status before child traversal, maps only leaves, propagates the ORIGINAL
root ID across children whose own root IDs differ, and defers unsupported type before quantity drift
before missing trigger. MARKET_PRICE intentionally maps to LastPrice, matching source rather than
inferring MarkPrice. Partial vectors are never returned on failure.

Transaction mapping retains separate commission/PnL/maker/RPI values and millisecond historical time
versus floored-second execution time. Negative direct-model time (-1.25 ms) produces -1 second, and -0
keeps its bits. One-way output uses the actual shared HistoricalTrade with positionSide Both; hedge
output defers that trade explicitly because the source route omits leg evidence. Optional PnL absence
stays absent. Typed mapper declaration inputs may carry infinity in pass-through numeric fields, which
is inspected natively before any JSON projection. Source info alias mutation and stable cloned native
order info are observed independently; frozen source page/row containers and unfrozen deferred records
are recorded separately.

## One compiling native control per new family

| Family | Named native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| declarations | PRIVATE-ORDERS native reduceOnly | 101 | 0 |
| helpers | PRIVATE-ORDERS native timeInForce | 101 | 0 |
| regular | PRIVATE-ORDERS native orderId | 101 | 0 |
| algo-decoding | PRIVATE-ORDERS native expected success | 101 | 0 |
| algo-mapping | PRIVATE-ORDERS native parentId | 101 | 0 |
| transactions | PRIVATE-ORDERS native execution timestamp | 101 | 0 |

Controls overlay production at a9048b651d2d19eff7a2276cc83ac379f01fed21: symmetric reduceOnly/isActivated Serde label swap; IOC->FOK
TIF; regular orderId+1; depth>=8 instead of >8; child-owned root ID instead of original root ID;
floor->trunc for execution seconds. Each compiled and reached its named assertion, restored saved bytes,
and passed its exact family test (1 pass / 6 filtered) with exit 0. Stopped at iteration **6**, zero
survivors in that floor; no further control matrix is claimed. The declaration control checks the native
boolean field directly; no wire roundtrip is claimed as measured.

The initial scoped check at d23a107647dfaefb3721b088d98eae899b1d08fe exited **101** because shared
HistoricalTrade has no blanket JsNumberOutput implementation. No tests or controls ran at that SHA.
Its extractor/target audit/fmt/census/oracle each exited 0, separately retained in the JSON. The source
fix exhaustively destructures all mapped-result fields and all 14 native HistoricalTrade fields, calling
the existing numeric-output checks on every leaf. No wildcard, local numeric wrapper, unchecked
passthrough, numeric Value projection or shared type/trait edit is used. New fields fail compilation
until explicitly checked. The first eligibility fix at 4265451f8f8b77cde4ae3fc048e1c97fb995e965 then failed its scoped check
with exit 101 for a redundant unused trait import under deny-warnings. Its separate non-cargo gates
passed and are retained. Removing that import changes no checks/values; fresh source gates above
are measured at a9048b651d2d19eff7a2276cc83ac379f01fed21. No earlier check is relabeled.

A premature survival comparison ran while the integration merge was still active, comparing the
source SHA to itself; its exit 0 is retained as a diagnostic and is NOT a merge-survival gate. After
the merge completed, the distinct merge SHA above was compared and exited 0 for the scoped inputs.
A read-only sample diagnostic during a delayed websocket-state launch exited 255 because that process
had already completed; no stack was captured. Its three tests and the completed full gate are measured as passed.
These diagnostics are separate from gate/control results.

Read-only sub-agent review checked production sequencing, typed/projection boundaries, all field/schema
assertions, 28 distinct notes/31 real primary/supporting targets and all six unique compiling-control
anchors. It found the draft bool sensitivity gap described above, corrected before source commit. It
ran no cargo/mutation/oracle/extractor/audit gate and made no edits; no runtime exits are attributed to it.

## Census and deliberate boundaries

Tomllib reads all 22 WOO/journal manifests with an independent 534-row expectation and a literal set
of 28 pending symbols. **28 moved: 9 pending->ported, 19->divergent**.
Before 113 ported / 192 divergent / 229 pending; after **122 / 211 / 201**.
The private source file has zero pending manifest rows after this slice. That is a census result,
not proof of arbitrary JavaScript parity or full exchange readiness. All 28 changed notes are distinct,
name their own target and justify the concrete CHOSEN adaptation for each divergent row. Seven exact
string unions and two direct enum mappings are ported. Three supporting types are explicitly mapped:
null/text amount union, owned typed-info Order projection, and regular type/TIF mapping result.

Dense retained JSON and typed native model inputs exclude executable getters/prototypes, sparse arrays,
lone UTF-16 surrogates and arbitrary extra property graphs. Source errors/readonly/frozen identities
become Result messages and owned public native records. Optional fields reject present null, except
the explicit regular amount and required transaction-PnL nullable types. Public numeric isMaker
construction is wider than the declared 0/1 union, though deserialization enforces it; source-compatible
numeric declaration fields are not silently restricted to runtime integer/minimum checks.

WooV3MappedOrder<T> is an owned operation-field projection with required typed cloned info, not a
shared Order adapter or arbitrary Order declaration ingress. It omits unrelated optional shared Order
fields, retains only the regular/algo operation shape, and uses strict captured record deserialization.
WooV3AlgoOrderMapping declaration ingress is therefore deliberately narrower than arbitrary Order[]
construction. Actual shared Execution/HistoricalTrade types are used without shared type/trait changes.
Retained-JSON ingress has its existing 128-level capture bound; runtime algo decoder additionally caps
its own child depth at 8. The typed recursive mapper has no claimed bound for arbitrary constructed trees.
No live route/store mutation/HOW row or additional shared type/trait changed.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for them.

## Final delivery refresh

Final read-only evidence review found no material issues and ran no runtime gates or edits.
Fetch and merge-down exited 0; actual merge `728ca64ee52921859ef6faa43013e54aff99c13e`
includes integration parent `fc23eb289f39c11f1bf30b56f1848254b2a520ac`.
At that merge, the implementation/tests/scripts/private manifest equal source gate SHA
`a9048b651d2d19eff7a2276cc83ac379f01fed21` (scoped diff exit 0), and all listed dependency
inputs including Cargo.lock equal the prior checked merge `5dd12405a62d525b9e8a71b7e90418227b61d67d`
(diff exit 0). No Cargo rerun is claimed at this merge. Fresh extractor and tomllib census both
exited 0 at `728ca64ee52921859ef6faa43013e54aff99c13e`; extractor text reports zero updates,
added or orphaned symbols. Census against that actual integration parent confirms 28 moves,
9 ported / 19 divergent, pending 229 -> 201 across the fixed 22 manifests / 534 rows.
The following evidence-only commit changes these two gate sheets; source gate attribution stays
at a9048b651d, and no unreviewed unrelated feature branch is an ancestor.
