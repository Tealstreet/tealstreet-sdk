# Blofin final seven rows

Branch `feat/rust-blofin-complete`; original integration parent
`a8a1322918ee9e6f036f876a7dab93bcb9ee16ee`.
Integration `faee40c0372da23d6558ad3da511c44a22b05331` was merged down at
`f716e8c8ba9a9f4c0d443ced7c55951381fa5c47` before the scoped gates.
Integration ancestors are inherited and not individually reviewed by this lane;
no separate private lane branch was merged. User opens the PR.

Independent tomllib census stops at **7 venue manifest files / 125 rows**:
**37 ported / 81 divergent / 7 pending → 40 / 85 / 0**.
**7 rows moved: 3 ported, 4 divergent**. File list, actual parent comparison,
independent literal expectations, exact changed symbols and targets are in
`complete-evidence/census.json`. Journal files are outside this venue census.
No empty mapped targets; no new parity exemptions.

## Measured gates

At **a351b504683f9fbcb8f3b7cac9024bcfc468dd3b**:

| Gate | Exit | Scope |
| --- | --- | --- |
| Cargo check | 0 | safe-cex-venue-blofin, all targets, jobs2 |
| Host Clippy | 0 | same crate, all targets, jobs2, --no-deps, -D warnings |
| Tests | 0 | complete:8 passed; orders:1 passed |
| rustfmt | 0 | changed source/test Rust files |
| Extractor check | 0 | text:0 updates,0 added,0 orphaned |
| TS coverage + Rust parity | 1 | TS66+14 declarations,0 drift; exactly3 inherited Rust findings |

Full commands/environment/exits/SHAs and logs:
`complete-evidence/gates-a351b50468/{native,static}.json` and adjacent logs.
Oracle compiler is `/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Rust-parity uses the existing binary; no Cargo rebuild of that checker.
Inherited findings left as directed: position_adapter.rs public
`BlofinPositionAdapterDeps`, `map_blofin_position_history`, `num` have no rs mapping
in this venue scope. This is not a green Rust-parity claim or workspace build.

## Bodies and honest boundaries

- **PORTED handlePositionTopic:** actual parent mapper, full position commit
  before original raw maintenance-margin/upnl sums, mapped-nonempty gate,
  roundUSD and NaN/negative-zero `||0`. Owned Position.info remains the already
  CHOSEN mapper boundary; aliases are not newly claimed to match. The coercion
  fixture proves positions commit before a later margin-sum failure.
- **PORTED listenOHLCV / listenOrderBook:** actual owned public facade; direct
  callback forwarding and returned cleanup Rc identity. All5 OHLCV fields have
  distinct explicit assertions; native actual listener registration/cleanup is
  exercised in addition to the independent forwarding spy.
- **CHOSEN handleOrderTopic:** immutable mapper ingress excludes source custom
  mapper mutation filledSize2→4, witnessed source amount0.5/native0.25. Dedicated
  private transient-status projection permits reconciliation of normal terminal
  rows while preserving original raw info; standalone mapOrders remains unchanged.
  Partial→filled/canceled/effective/order_failed fixtures assert retained native
  Closed/Canceled and Store status inside fill callbacks. Fills go through the
  shared `create_incremental_fills` with cumulative filledSize and
  filledSize×averagePrice, as the source does: repeat, regression and a replayed
  filled frame emit nothing, each partial carries its own price without float
  noise, and a non-finite filledSize is skipped. Emitter failure, contract NaN
  and strict timestamp threshold/floor have decoded native assertions.
- **CHOSEN handleAccountTopic:** info is an owned copy. Source caller mutation
  changes stored info100→999; native retains100. Free/total use current used/upnl.
- **CHOSEN private class:** static concrete handlers; source runtime replacement
  preserves balance.total20, native actual account handler produces97.25. Timer,
  dedupe and all prior control functions are genuinely composed. Private-only
  disposal blocks future dispatch while parent and public socket stay usable.
- **CHOSEN exchange class:** default methods use concrete static bindings rather
  than writable source instance fields; source replaced mapper returns
  runtime-replacement while native default reader returns empty. Owned options
  remain livenet after caller changes its copy; source live options select demo.
  Weak parent callbacks avoid Rc cycles: retained native child rejects after owner
  drop; source child retains parent. One Store/XHR/cache context, actual API-factory
  function, signed native requests/HMAC/default20s budget, shared auth streak,
  inherited backoff scheduling, parent3-route timer read and disposal order are
  independently asserted. Common base/HTTP/socket runtime remains explicit hosts.

The oracle executes the maintained **exchange and private classes**, actual
createAPI initializer and inherited getTickInterval method. Its public socket is
an intentional venue forwarding spy. Native composition owns the actual public
facade, whose source body parity was covered in the prior public-stream slice.
This is **not** proof that all3 maintained classes execute together. Class71/16
member inventories are surface floors; runtime enumeration stops at66 source
cases and the explicit native composition/reentry/lifetime probes. Existing
per-function manifests retain their prior specific divergence classifications.
Native models, f64 bits, errors, dedupe, status-at-fill, callback/cleanup identity
and requests are asserted before effect comparisons; no decode/re-encode proof.

Clean static sub-agent review bound to **a351**, with no reviewer gate runs:
`complete-evidence/review.json`. Initial production findings and later test gaps
were fixed, not excused by ownership notes.

## Discrimination control

`complete-evidence/controls-a351b50468/controls.json` binds the original SHA,
commands, mutation anchors, before/after hashes and restoration to a351.
One **compound8-production-mutation** run compiled, then all8 named native tests
failed at runtime: **exit101**. Restored source was byte-identical, scoped source
`git diff --exit-code` **0**, then all8 passed: **exit0**. Compiler refusal is not
counted as discrimination. This is aggregate discrimination, **not** independently
isolated per-mutation coverage or completeness; masking remains a limit. No tested
survivor was dropped. Negative/restored raw logs are adjacent.

## Earlier failures and unrun gates

- f716: check0, Clippy0, tests7+1 pass, fmt0, extractor0. Source coverage passed;
  Rust-parity harness exited1 on a missing linked crypto dependency. Corrected the
  harness's dependency links; no gate exemptions were added.
- 2734: check101/Clippy101/tests101 from a test calling a crate-private ingress
  constructor; fmt0. Fixed by public numeric ingress. Its separate static driver
  raced the next commit; that measurement is explicitly INVALID in static.json
  and covers neither SHA. Stable-SHA runs supersede it.
- 085: check101/Clippy101/tests101 from omitted PositionRecord import; fmt0,
  extractor0, scope1 for exactly the3 inherited findings. Added explicit import;
  broad wildcard imports were rejected by the strict indexer, not exempted.
- Dirty development failures/logs are retained under complete-evidence/development
  and never attached as final gates. Development count57→60 mismatch is not a
  mutation control; a cadence assertion that ignored signed-success streak reset
  was corrected to assert reset plus real rejection-driven backoff.

Not run: workspace builds/suites, full crate test suite, dependency-inclusive host
Clippy, wasm compilation/Clippy, live HTTP/socket/venue integration, CI reruns.

## Final merge and gates

Integration **2691078b11e3bff6d4541ede634a3ed1e54627ab** merged down at
**0ec1dc498cea6b84c167dd6eab37e00c4bb1eba6**. Scoped source, Cargo inputs,
shared dependency sources and venue manifest diff from a351 exits **0**;
`complete-evidence/final-merge-preservation.json` carries the exact command.
Only the oracle opening comment changed to describe its intentional Public spy.

All final gates were remeasured at **0ec1**: check **0**, host Clippy **0**,
complete8/legacy-orders1 tests **0**, fmt **0**, extractor **0** (zero updates),
canonical TS coverage zero drift66+14, Rust parity **1** for exactly the same
three inherited position-adapter findings. Commands/exits/raw logs are in
`complete-evidence/gates-0ec1dc498c/`; no gate was still running at delivery.
Clean static preservation review is bound to0ec1 in
`complete-evidence/final-review.json`; reviewer ran no gates and excluded evidence
documents. The final evidence-only commit changes no gate inputs.
