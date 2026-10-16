# Webworker finding audit

Baseline: ac0f5fccf24207b26c279e67da2a88f2c72a7aae. Python tomllib
read all 38 webworker manifests, stopping after the final tracked manifest:
222 rows, 115 ported / 107 divergent / 0 pending. All 107 divergent notes
start with a canonical bucket word: 55 CHOSEN, 46 MIXED, 4 FINDING, 2 FORCED.
Twenty omit the colon after that word; this slice normalizes punctuation.
This census does not establish behavioral parity of all 107 implementations.
Classification follows [the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

Three leading notification row notes were stale after the shared Store cutover.
The leverage helper now returns StoreMutationError unchanged but retains a
witnessed FORCED symbol boundary under
[the canonical ruling](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
Both position-refresh rows retain their explicitly named native task/input
choices; they become CHOSEN. The removeOrders findings embedded in both
CallMethodHandler rows are also stale. The native notification witness asserts
committed state before notification, stopped fanout, recovered error metadata
and the failure reply; outer full/partial dispatch now has independent cases.
The source corpus stops after 36 cases, the native corpus after 32; source BBO
accounts for the four cases without a native synchronous-throw equivalent.

The shared OrderRiskProfile serializer already repaired property enumeration,
but the worker test still asserted the old difference. Baseline cargo failed
that stale assertion at exit 101. The replacement compares property order only
after native numeric assertions. The risk row retains its distinct cache choice
and no longer claims an open field-order defect.

## Residual findings; no closure claimed

- OrderBookSubscriptionsHandler.handle: core Exchange::listen_order_book's unit
  callback cannot return the synchronous subscriber failure. The repaired leaf
  returns its original Store failure, but the throttle adapter still projects it
  into a host exception report. Owner: shared exchange listener ABI. Direct leaf
  propagation is not proof of propagation through this parent. Its JSON-valued
  payload also projects native NaN metadata to null; the actual-source leading
  throttle call forwards NaN. Both mechanisms stay findings on this symbol.
- ChaserOrderManager.findBestPrice: shared Store dictionary writes cannot retain
  the prototype introduced by an earlier __proto__ market assignment. Owner:
  shared Store symbol-map representation. tests/chaser_price.rs retains the witness.
- UiStateHandler.handleActiveMarginSymbolChange: custom inherited fetched-leverage
  entries are absent from the shared typed map. Owner: shared Store dictionary
  representation. tests/ui_margin.rs retains the distinct stock-prototype cases.
- DisplayedPositionsPostprocessor process/append: inherited __proto__ empty objects
  and a ticker prototype write remain distinct from the declared callable-output
  boundary. Owners: shared Store writes and displayed-position projection.
- TwapInstance.createNewState and TwapPostprocessor.process: ordersSizes[-1]=NaN
  is a named array property not held by Vec. The postprocessor also conflates
  an own origin:undefined with absence. Owner: TWAP state/presence representation.

These seven affected symbols are an open-finding floor, including defects
embedded in MIXED notes; the leading FINDING count alone omits them. The audit
does not silently turn these defects into representation choices.

## Classification follow-up

The surrogate-related log, health-reason and market-remainder notes need their
program-observation scope checked under
[the canonical ruling](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
Their existing escaped source captures do not by themselves prove diagnostic-only
use. This slice does not promote those rows or claim a complete consumer audit.
Owned snapshots, output refusals and numeric ingress use the canonical anchors
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), and
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect); no declarations are copied here.

The subsequent leaf repair changes the worker-local post carrier and return type;
no shared type or trait changes are included in this audit.
Measured gates, controls and omissions are recorded separately in FINDING_GATE_RESULTS.json.

## Measured gates

At 3f9dc68037215356435aeac164ac9be9592224e6: six selected tests
(store_notification, risk_postprocessor, ui_state), scoped all-targets cargo
check, extractor check and source-restoration diff each exited 0. The live
source notification supplement independently measured 36 cases and 58 subscriber
calls at exit 0. Four compiled controls stopped at the listed mutations: each
failed 101 at its named assertion, then passed restored at 0, with no survivors.
Details, commands, baseline failure and the writer overlay are in the tracked JSON.
The production source diff is restored, including the temporary shared serializer
mutation. No shared production type or trait changed in this slice.

Current tomllib after the sub-agent corrections: 114 ported / 108 divergent /
0 pending. Movement: 0 pending->ported, 0 pending->divergent, 0 divergent->ported; one ported->divergent constructor
correction names the new host dependency and regular-post report boundary.
Four former leading FINDING rows are reclassified after the repairs; zero
leading FINDING rows remain, plus the
residual floor above. The historical census in the JSON at 3f9 was 116/106/0;
that ported classification was rejected by review and is not retained.
Not run: full webworker suite, workspace check/tests, host or WASM clippy, WASM
tests, TypeScript typecheck, live browser/sockets or CI.

## Leverage boundary correction

The actual source DefaultStore/helper matches a materialized U+D800 symbol,
commits leverage8 and notifies while retaining the same unit. Source type/unit
checks run before serialization. The Rust String and whole Position decoders
refuse that source-accepted input; replacement, paired-astral and literal
backslash-u inputs commit and notify natively. This is a separate cause from
the repaired notification failure, so the row remains divergent/FORCED. The
new source witness stops after four symbol cases. Replacing D800 in the actual
helper's filter in memory fails the named source commit assertion. At 857cfa24f282f6ab674c8a91844ec64e066933d8, the source filter mutation
exited1 at the named commit assertion and restored source exited0; the changed
native store_notification target exited0 (two tests), and extractor check exited0.
The tracked JSON contains commands and the four-symbol floor.

## Order-book leaf repair

The leaf sends the original book through a mandatory native sink and returns post
or Store errors. Its Store branch retains the original mutation variant and
subscriber cause. The parent unit callback still reports that returned failure,
so its finding remains open. A separate four-case actual-source floor pins native
nonfinite values, post failure before mutation, reentrant book mutation and nested
metadata aliasing. The native owned metadata difference is retained as CHOSEN
with the canonical snapshot anchor. A separate actual-source metadata post retains
a materialized U+D800 tag; the native whole metadata decoder refuses it. The leaf
therefore remains MIXED with that witnessed FORCED text boundary under the
canonical surrogate anchor; no row is promoted to ported. The constructor row becomes CHOSEN for its
mandatory host dependency and regular-post error reporting. The leaf accepts a
shared native book reference, preserving caller aliases as well as reentrant post
edits; the parent wraps the existing owned Exchange callback value. New production
records are confined to this worker crate. Fresh gate exits are recorded separately
when measured, rather than extending earlier exits to the new source.

At 86a69d077c3c9eed6f562d2e2d7b464e31b32075 the live Source supplement, seven
selected tests (subscriptions/store_notification), scoped all-targets check,
extractor check and source-restoration diff exited0. Both compiled controls
failed101 at their named leaf assertions, then passed restored0; no survivors,
stopping after two controls. The Source floor is36 ordinary cases/58 subscriber
calls, four symbol cases/four calls, four BBO leaf cases/four posts, and one
materialized metadata case/one post. Detailed commands and omissions are in
FINDING_GATE_RESULTS.json. The writer after manifest corrections exited0 with
zero updated manifests and zero added/orphaned symbols.

## Prepared metadata correction

Review found the owned-alias note did not cover metadata number loss. The leaf
now consumes the existing JsJsonValue carrier, retaining native metadata numbers
through the mandatory post/clone sink. Four Source/native cases stop after NaN,
both infinities and negative zero, with four post calls on each side. A separate
actual-source handle/throttle route forwards NaN metadata; the native parent's
JSON payload already contains null. This remains a numeric finding on handle
under [the canonical requirement](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).
The leaf repair does not conceal that parent ingress gap or the unit callback.
Fresh exits for the prepared carrier are recorded separately from86a69.

At 74b4e9e7970bf0d4d3ca048b7f3f2d5c7d3f788d the prepared metadata writer,
Source supplement, eight selected native tests, scoped all-targets check,
extractor check and restoration diff exited0. The one planned compiled metadata
control failed101 at `BBO native metadata number variant`, then passed restored0;
no survivor. The four additional numeric metadata cases make four post calls;
the actual Source parent NaN route makes one post. Final sub-agent review was
clean and ran no gates; extractor completed after that review. Previous compiled
book/Store controls remain evidence about86a69, rather than being relabelled as
measurements of this new carrier. The omitted-gate list above remains unchanged.
