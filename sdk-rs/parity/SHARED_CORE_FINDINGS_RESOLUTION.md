# CrossTrade, transport and Store finding resolution

Parent actually used: integration 333fd419fdbf70f081039610f22da9ee653bc96f.
The original finding tables remain immutable at PR 7937 commit
702fc59dd864719b85af90c2dde5c7bd5003a241 and PR 7940 commit
0e2cde6a073f254ce0140ce4d064fc0deebd52c6. Neither feature branch was an
ancestor at the resolution freeze. SHARED_CORE_FINDING_TYPES.md records the
historical type split and independent 26-row checker-contamination review.

## Resolution scope

The independent expected population is 46 previously new explanation findings:
all CrossTrade iterations 1–26, transport 1–10 and 12–15, and Store 1–4 and
6–7 in those frozen tables. All were re-examined against actual TypeScript and
native call sites. No status or Rust target changes are intended. Finding
resolution updates the explanation, not the historical audit result.

| Group | Rationale only | Direct code repair plus rationale | Rows examined | Stop iteration |
| --- | ---: | ---: | ---: | ---: |
| CrossTrade | 20 | 6 | 26 | 26 |
| Transport | 14 | 0 | 14 | 15, excluding supported 11 |
| Store | 1 | 5 | 6 | 7, excluding supported 5 |
| Total | 35 | 11 | 46 | Frozen selected cohort |

The eleven direct repair rows are Store updateInArray, updatePositions,
updatePosition, updateOrder and updateMarket; CrossTradeConfig and
CrossTradeCopier; both symbol helpers; and both manager position audits.
Containing class/factory rows are not multiplied into additional code defects
merely because they compose those operations. This is a direct-row partition,
not eleven independent root causes. All eleven still have remaining selected
representation or ownership costs, so a code repair does not imply ported status.

The 35 rationale-only decisions are newly explicit owner choices, not a claim
that the previous annotations already justified them. They are bounded static
reviews of these interfaces, not complete function-conformance proofs. No
additional admitted-domain defect was established in transport's selected 14.

## Concrete repairs

Store's ordinary JSON-map patch decoder rebuffered an admitted info object whose
only own key was `$serde_json::private::Number` and string value was `17`, changing
the object to a number. Info now clones its actual Value directly; numeric and
known-field validation paths remain distinct. The native test asserts object
kind, literal key/value and untouched containing-row siblings across seven
operation/carrier cases before executing the maintained source Store witness.
These are JSON-map and ModelPatch carriers for three individual operations plus
one batch operation; there is no claim of eight distinct paths. The source
witness checks actual retained object identity on four public operations.
Ticker strips info in both runtimes and receives rationale only.

CrossTrade's two public closed weight-bearing types now capture numeric input
before their serde field decoders. Exhaustive eligibility implementations cover
every field, nested copier, map and enum. Tests assert native master and copier
bits for eight independent raw numeric tokens through bare and owned-Value
ingress, including signed zero, overflow and underflow. Source JSON.parse bits
are asserted independently before any output projection. Borrowed-Value,
shared gateway, arbitrary depth and all possible malformed-shape routes are not
claimed by this finite numeric test.

The symbol helpers now return the existing TableHit identity rather than only a
String. Own shadows and empty-string fallback remain distinct from inherited
constructor/function and Object.prototype hits. Reverse inversion preserves
the primitive __proto__ setter omission. The manager retains these identities
through relevant-position filtering and market selection, preventing orders
where source skips them. Native helper assertions distinguish exact identities
from Missing. The 26-case source/native position corpus includes three negative
prototype branches and a positive own-shadow order branch. The original normal
twelve mapping cases remain in the existing suite.

Canonical rationale and its price are recorded only at
[Store patch choice](MANIFEST-SPEC.md#store-typed-patch-choice),
[CrossTrade capabilities](MANIFEST-SPEC.md#cross-trade-native-capabilities),
[CrossTrade diagnostics](MANIFEST-SPEC.md#cross-trade-diagnostic-choice) and
[relay scope](MANIFEST-SPEC.md#transport-relay-policy-scope), with row-local
citations. Shared declarations remain at
[§14](MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](MANIFEST-SPEC.md#divergence-taxonomy) and
[§19](MANIFEST-SPEC.md#forced-surrogate-conflict).

## Review and evidence limits

Independent sub-agent inspection accepted the Store object/numeric repairs and
the ordinary-object stock-prototype symbol implementation. One reviewer
withdrew the earlier proposal to justify wrong order branches as an own-string
ABI choice: existing identity tags can preserve those branches. Another caught
the repeated batch test case; its independent path count was corrected to seven.
The source helper invocation was also corrected to its actual options-object API.
No reviewer ran Cargo or tests concurrently with the root gates.

Actual exits, exact tested SHAs, deliberate controls and exclusions are recorded
separately in SHARED_CORE_RESOLUTION_GATE_EXITS.md. A baseline failing regression
is distinct from a deliberate mutation. No checker false positive supplies a
runtime or annotation finding here. No TypeScript production body changed.
