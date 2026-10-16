# WOO divergence sample and signer repair

Gate SHA: `11673274d9e889e05421bf3c21f323c143b659a2`.
Branch: `fix/woo-signing-domain-audit`.
Parent: `0da82b9f68d2745e6f6c8107159f6bf40ebcd130`.
Integration `d19d2874e0265c85602b9240d618612c65984c02` was merged down before the
baseline push in merge `f09df872ecc574b64cd47bb2e0edcbf7193da4e3`.
The parent carries reviewed unmerged WOO PRs #7858, #7862, #7867, #7876, #7880,
#7883, #7894, #7899 and #7903. Kucoin response repair #7889 is separate.

Tomllib census: **218 ported / 314 divergent / 0 pending**, of 532 rows.
Status moves in this slice: **0**. Two existing divergent signer rows now open
with FINDING for their remaining complete-config omission. The three leading
open FINDING rows in this WOO branch are signV1, signV3 and WOOXExchange.
Historical occurrences of the word FINDING in repaired notes are not counted as
open rows. The earlier 308-divergent/no-FINDING census was under-audited; this
sample supplies concrete counterexamples rather than certifying every row.

## Static sample floor

Nine rows across eight maintained-source manifests were inspected against their
Rust counterpart. The first eight ended at the ticker handler; the directly
related signV1 follow-up is iteration nine, where this enumeration stops.

| Row | Source observation and native cost assessed |
| --- | --- |
| normalizeSymbol | lodash memoize exposes mutable cache; native stateless function omits it as declared |
| WooV3RegularWriteTarget | structural readonly union permits overlapping declaration shapes; native exclusive ID enum rejects both IDs at declaration ingress as declared |
| WooV3CompletePositionModeEvidence | source literal/readonly declarations are narrower than public Rust construction; note identifies that wider construction |
| WooV3PublicParityMismatch | source retains diagnostic values/provenance; native immutable owned handles and required String boundaries are declared costs |
| WooV3InvalidModeSelection | source retains arbitrary requestedMode; native owned JSON and the recorded actual returned-unit boundary are declared costs |
| WooV3AuthoritativeTickerInfo | source retains BBO/Trade referents and freezes info; native copies them into mutable owned info as declared |
| signV3 | previous JSON-compatible-config CHOSEN note omitted ordinary JSON exception-path defects and proxy context loss; repaired/priced below |
| WooPublicWebsocket.handleTickersStreamEvents | source copies raw fields to Store; native typed numeric writer's omitted patch/refusal and inherited normalization costs are declared |
| signV1 | same unpriced method/URL/proxy problems as signV3; repaired/priced below |

Seven sampled notes identify real differences supported by source and Rust
inspection. This is not proof that those notes exhaust every cost. Their full
historical tests were not rerun in this slice. In particular, the recorded
surrogate observations are not remeasured here or promoted into new claims.
The remaining divergent rows were not enumerated or certified.

## Concrete repairs and remaining omission

Previously, an ordinary JSON method object with non-null non-callable
`toUpperCase` silently selected GET. Source throws before signing. Non-string,
non-null URLs were silently accepted instead of failing startsWith. Both native
signers now preserve those source failure messages; missing/null method members
still short-circuit to GET. Native String error results do not carry TypeError
brand/stack; only source brand is observed by the source case.

The provider projection previously retained only method/url/params and uppercased
V1 method spelling. It now forwards the original V1 spelling (pOsT), normalized
V3 spelling (POST), original absolute URL, params, logical untransformed body,
string-valued headers, string baseURL and representable integral timeout.

The complete mutable Axios config is still absent from the typed provider ABI.
The actual source callback observes custom=97; exhaustive native RestRequest
field assertions show only the independently expected known values and absent
extra native fields. Unknown config, callback-driven mutations, unsupported raw
header/baseURL/method/timeout values, optional absence/null distinctions and the
negative-zero timeout sign remain explicit FINDING costs. The typed i64 timeout
maps -0 to 0. Those paths were not repaired or represented as matching. The
custom-field runtime observation is paired with exhaustive native carrier
inspection; it is not an assertion on a projection that silently drops fields.

Native observations retain actual RestRequest clones. Assertions inspect their
typed fields and body phase before the comparison view is built. The view uses
exhaustive struct destructuring without a remainder, so adding an ABI field
requires revisiting the observation rather than silently hiding it. Literal
expectations distinguish body37, query41, header53, timeout71 and source custom97.
Per-case proxy invocation deltas are independently 0 or 1, total two, preventing
a duplicate side effect from collapsing into the same last captured request.

Canonical declarations are referenced rather than duplicated:
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`,
`sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`,
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`, and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`.

## Measured gates

| Gate at the gate SHA | Exit and observed result |
| --- | --- |
| Scoped cargo check, WOO all targets, jobs 2 | 0 |
| cargo test --locked, signing target, jobs 2, one test thread | 0; two tests, including 22 actual-source/native audit cases |
| node signing-oracle.cjs --check | 0; 37 maintained HMAC/Axios captures, no network |
| parity:extract:check | 0; Would update 0 manifests, 0 added, 0 orphaned |
| node --test --test-name-pattern=WOO scripts/rust-ci-wiring.test.mjs | 0; two passed |

The reconciliation writer reported zero additions/orphans. Cargo ran serially
with the TypeScript compiler environment supplied. The existing mandatory WOO
signing target invokes the new source probe; no root target was removed or CI
rerun requested. Original capture fixtures were read and checked, never rewritten.
The source probe reuses the existing actual-signer loader with an asserted
boundary marker and actual HMAC/Axios dependencies; it does not fork venue logic.

Baseline `f09df872ecc574b64cd47bb2e0edcbf7193da4e3` exited 101 at compile time
because the fixture used the response-body method name on RestBody. It is not a
semantic discriminator. Corrected baseline `e0a965ed4dab38f52a6e5a3a54c6852fe065ef0b`
compiled and exited 101 at WOO-AUDIT native optional method must throw, before
source execution. The standalone actual-source 22-case probe exited 0 and
observed both source method errors, URL errors and complete proxy field values.
Only the final five gates above are attributed to the repair SHA.

Sub-agent review required independent proxy call counts and native method/URL/
params assertions, plus explicit signed-zero/non-number/presence pricing. These
were added; follow-up found no remaining blocker in the reviewed repair. Review
ran no Cargo and did not certify the unenumerated divergent rows.

Five isolated controls each compiled and exited 101 before source/wire comparison:

| Mutation | Named native failure |
| --- | --- |
| Non-callable optional method defaults to GET | WOO-AUDIT native optional method must throw |
| Non-string URL is silently accepted | WOO-AUDIT native nonstring URL must throw |
| Provider body becomes null | WOO-AUDIT native proxy body |
| Provider is called twice | WOO-AUDIT native proxy invocation count |
| Provider V1 method is uppercased | WOO-AUDIT native provider method spelling |

Enumeration stops at V1 provider spelling. No survivor or compiler-only failure
was counted in this finite set. Exact bytes were restored; the full signing
target passed again (exit 0, two tests). Scoped src/tests/scripts preservation
against the gate SHA exited 0. Before adding this manual receipt, the full
tracked preservation diff against that SHA also exited 0. Gates are not
reattributed to the receipt descendant.

Not run: all other WOO target files or full CHOSEN-note census, workspace
compilation, host/wasm clippy, full monorepo suites, live exchange calls, CI reruns,
other venue gates, full mutable transport-config proof, or new surrogate proofs.

## Merge-down repair for the conflicting audit PR

GitHub REST reported PR7909 mergeable=false/mergeable_state=dirty at its pushed
8885242781d50b81065d0b749d8b2d9db20d3da8 head. Current integration
49a942e76143d6d1d44567edff59c12669d92627 merged as
9f4ef005845fe3d206ffbfcfdd6aa533825e83b9, which was pushed before fresh gates.
The actual merge base was d19d2874e0265c85602b9240d618612c65984c02, one base.
No merge-tree dry run is used to claim GitHub acceptance.

The only manual conflicts were rust_files inventories in the two WOO socket
manifests. Official parsing/serialization preserves the merged rows and unions
both inventories with actual mapped row paths, without duplicates. Incoming
exchange/socket host-capability annotations are retained. The official
parity:extract reconciliation writer exited0/Updated0 manifests,0 added,
0 orphaned; no rows were regenerated. Shared declarations remain cited by the
canonical anchors above; no rule is restated by the resolution.

Independent tomllib measures218 ported /314 divergent /0 pending,532 rows across
21 manifests, stopping atwoo.ws-public.ts.toml. Zero statuses move versus888524.
The three leading FINDING rows signV1,signV3,WOOXExchange remain. The prior nine
row CHOSEN audit is still a sample, not certification of all314 divergent rows.
The Source and Rust WOO src/tests/scripts preservation diff against888524 exits0.
Incoming shared-core changes are freshly compiled below; the broad integration
ancestry is not claimed as this lane's newly ported work.

A read-only sub-agent verified both exact inventory unions and the expected
three-way result for all532 rows, with no annotation loss. Its scoped whitespace
check exited0. Its additional broad whitespace check exited2 on111 files;
inspection showed every one byte-unchanged from incoming integration. That
upstream whitespace result is retained, not relabeled as a passing full-tree
check. The reviewer ran no Cargo, tests or extractor.

All fresh measurements below apply to pushed merge SHA
9f4ef005845fe3d206ffbfcfdd6aa533825e83b9:

| Named gate | Measured exit |
| --- | --- |
| cargo check -p safe-cex-venue-woo --all-targets --jobs2 | 0 |
| exact signing target, --locked --jobs2, one test thread | 0;two tests including22 native/source audit cases |
| node signing-oracle.cjs --check | 0;37 live HMAC/Axios captures, no network |
| parity:extract:check, output inspected | 0;Would update0 manifests,0 added,0 orphaned |
| node --test --test-name-pattern=WOO scripts/rust-ci-wiring.test.mjs | 0;two checks |
| WOO source/tests/scripts and maintained-source preservation vs888524 | 0 |
| scoped WOO manifest whitespace check vs888524 | 0 |
| compound(path,kind,ts) census/identity/status preservation | 0;532 identities,218/314/0,zero status changes |

The original five mutation/control results are not remeasured or reattributed
to this merge SHA; their11673274 evidence above remains separate. Current
unchanged production is freshly compiled/tested here. No other WOO exact target,
full CHOSEN-note census, full venue/workspace/monorepo suite, host/wasm clippy,
live credentialed venue behavior, browser check, or CI rerun was run.

A subsequent GitHub REST read at9f4ef00584 reported mergeable=true and
mergeable_state=unstable. This establishes conflict clearance, not CI success
or a merge. The documentation-only receipt descendant must preserve a full
tracked diff against9f4ef00584 excluding this receipt before carrying its gates.

## Symbol and numeric-response batch

Branch: `feat/woo-symbol-normalization-audit`.
Integration parent: `35ef350885be6954cd4ac43d7c9239f4d8385bf9`.
This parent already contains the preceding Kucoin formatter and WOO work;
there is no additional unreviewed ancestor branch in this slice.

Eight existing divergent rows are audited here, ending at
`getTransactionHistorySymbolCandidates`. This includes a re-audit of the earlier
`normalizeSymbol` sample; eight is a batch size, not eight newly unique rows.
The other 306 divergent rows are not certified. Tomllib observes WOO
**218 ported / 314 divergent / 0 pending**, 532 rows across 21 manifests;
Kucoin remains **149 / 42 / 0**, 191 rows across 12 manifests. Status moves: **0**.
Existing row identities, order and statuses are preserved by the manifest parser
and serializer; this does not regenerate the source inventory.

| Audited row | Classification and observed cost |
| --- | --- |
| normalizeSymbol | CHOSEN; real lodash cache injection changes the source result; native wire normalization stays stateless |
| reverseSymbol | CHOSEN; real lodash cache injection changes the source result; native wire normalization stays stateless |
| parseFiniteFundingNumber | CHOSEN; JSON input and fixed intrinsics; changing source String.prototype.trim admits a blank string and yields zero |
| normalizeTimestampSeconds | CHOSEN; executable source numeric hooks yield 1234 seconds and retain thrown sentinel identity; native JSON has no executable hook |
| normalizeTimestampMilliseconds | CHOSEN; executable source numeric hooks yield 1234750 milliseconds; native JSON has no executable hook |
| normalizeTransactionSymbol | CHOSEN; normalization inherits the mutable source cache result |
| normalizeRequestedSymbol | CHOSEN; default typed entry retains stateless policy; the new fallible callback entry repairs the omitted method seam |
| getTransactionHistorySymbolCandidates | CHOSEN; reverse cache changes the first source candidate; native literal order remains independently pinned |

Taxonomy totals within this batch: FORCED 0, CHOSEN 8, MIXED 0, CORRECTED 0,
FINDING 0. The notes now state why each smaller API is retained. This does not
turn the existing three leading WOO FINDING rows into choices or close them.
Mutable cache injection is observed behavior, not a performance-only omission.
Native NaN returns undefined in the three source numeric helpers; unsupported
input representation alone is not evidence of an unequal numeric output.

The requested-symbol repair supplies a fallible callback at exactly the final
method-call site. Native assertions pin the uppercase stripped argument, arbitrary
returned Rc identity, original error Rc identity, no call for absent/empty raw
input and one call with an empty stripped suffix. The default wrapper remains
callable by existing typed consumers. The callback caller must perform the live
method lookup/call; this extracted body does not assemble the complete WOO class,
wire its private fields or provide an executable JavaScript string realm.

The existing response oracle has an audit mode that executes the maintained
source members and actual lodash memoize. It checks twenty-four independent literal
observations, ending at `requested-empty-suffix`; no generated fixture is written.
All new native field/identity/bit assertions precede spawning this source process.
The unchanged response capture still checks 487 cases, and the utility target
checks the existing 1557 cases. The root WOO command now mandates both native
targets, with its exact wiring assertion updated in the same commit.

Canonical declarations:
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`,
`sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`,
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`, and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`.

Fresh final build, host clippy and canonical wasm clippy exits are recorded
outside the repository with the exact final commit, rather than transferring
historical exits in this document to a later SHA.

### Discrimination and review

Controls ran against `3b3b803f70`, with isolated temporary mutations and exact-byte
restoration of both maintained source files. Enumeration stops at source symbol
swap: three compiling/executing controls, zero survivors.

| Control | Observed failing assertion | Exit; restored exit |
| --- | --- | --- |
| Pass unstripped raw text to requested-symbol callback | WOO-RESPONSE-AUDIT native requested | 101; 0 |
| Invoke requested-symbol callback twice | WOO-RESPONSE-AUDIT native overridden argument | 101; 0 |
| Swap maintained TS normalization base/quote concatenation | WOO-RESPONSE-AUDIT normalizeSymbol-cache-delete | 1; 0 |

The first control driver initially expected a later native assertion; the
compiled test instead failed earlier at the default wrapper's requested-symbol
assertion. The receipt names that actual failure. This was a driver expectation
correction, not a compiler error or a surviving mutation. The isolated native
restores each passed one targeted audit test; the restored source probe passed
all twenty-four observations. These measurements are not reattributed to a later
commit containing this explanation.

Two read-only sub-agent reviews found no callback implementation blocker. One
required explicit source restoration checks for all cache-dependent helpers;
those four additional assertions were added and the follow-up review cleared.
Neither reviewer ran Cargo. Complete intrinsic overrides and owning-class
installation remain unproved; the twenty-four observations are a finite floor.

## Class binding and regular-write audit batch

Branch: `feat/woo-class-response-audit`. Integration parent:
`c5ee2969e91aa74887dc492a4b2347c4e81c2bb5`, the user's merge of the preceding
slice onto `02b0ce7eafda0d544dcd10cc8756c8b58ed74a06`. Origin was fetched and
merged first. The fresh branch then used that identical-content integration
merge, avoiding a parallel merge history. No extra unreviewed lane ancestor is
introduced. Previous gate exits are not transferred to this branch.

This batch inspects eight previously unaudited rows, ending at
`decodeMutationResponse`. Seven had documented representation/ownership choices;
one had an unpriced source-contract callback defect, now repaired. Every
retained CHOSEN note now states the API tradeoff rather than only listing its
representation. This is a bounded audit, not certification of all 314 rows.

| Row | Audit disposition |
| --- | --- |
| mapPlaceType | CHOSEN captured input/closed result; no additional dense-data defect found |
| buildWooV3RegularPlaceRequest | CHOSEN owned descriptor and off-signature flag refusal |
| buildWooV3RegularAmendRequest | CHOSEN captured order/request and off-signature property refusals |
| buildWooV3RegularCancelRequest | CHOSEN owned request/query in place of frozen referents |
| buildWooV3RegularCancelPendingRequest | CHOSEN Option string/owned request boundary |
| decodeSuccessData | CHOSEN captured response/error ownership |
| decodeWooV3RegularPlaceResponse | Source-contract defect repaired: live Set.has decision/error; residual captured output/input costs remain CHOSEN |
| decodeMutationResponse | CHOSEN closed status/owned result; separate native out-of-domain panic repaired |

Running discovery split for these two audit slices: **16 unique rows; 14
choice-only observations and 2 source-contract defects, repaired**. The first
batch's requested-symbol method-call defect counts here even though its residual
cache boundary retained a CHOSEN bucket; final bucket totals are not discovery
totals. The second defect-bearing row is decodeWooV3RegularPlaceResponse. The two
symbol rows revisited for class adoption are already in the initial eight and
are not added to the denominator. The earlier signing audit is a separate,
overlapping historical sample and is not added here. The native BAD-status API
hazard is reported separately because BAD is outside the TypeScript status type;
it is not inflated into an additional on-signature source defect.

### Actual class adoption

Two original divergent rows move to **ported**, targeting the installed
`response_class` bodies. The old `response` wire facades remain available to
typed consumers and are explicitly excluded from these canonical class targets.
The owning WOOXExchange row stays FINDING: the other 66 own arrows, prototype
binding and metadata-state assembly remain incomplete. Imported normalizeSymbol
behavior remains priced on its own utility row.

The new class bodies use compatible value/property/call/RegExp capabilities,
retaining the source operation order and arbitrary dynamic values rather than
converting the input through Rust String. Class initialization installs both
arrows directly and the test rejects routing either through unbound_member.
Two whole constructions complete client assignment, proxy creation, public and
private socket constructors, and fill subscription. Native assertions inspect
all four client identities, socket owner identities, fresh arrow identity,
default values, lexical receiver despite an alternate call receiver, override
arguments and returned/error referents before invoking the entire maintained
TypeScript class. The eight default cases end at PERP; override checks end at a
distinct getter failure. Getter1/method0 is independent on both sides.

The first review caught a false-green instrument: method and getter failures
used the same sentinel. A bypassed getter could pass by calling that method.
Distinct sentinels plus getter/method call counts replace that witness. This
finding concerned the proof, not a demonstrated production body mismatch.

### Live membership and native status refusal

Source `ReadonlySet<number>` permits a live has override: a Set containing 81
with has returning false permits a matching response. A throwing has preserves
its original exception and wins before later type/quantity reads. The canonical
native decoder now accepts a fallible membership callback and returns its
original error value in a generic Membership variant. The slice facade delegates
with ordinary membership. Native literal assertions inspect the decoded result
(type, three decimal strings and distinct order/client/level/timestamp numbers),
queried ID bits, call count, original error Rc identity and zero callbacks after
earlier invalid-ID/request refusals. Five actual-source cases end at wrong
request; source has getter count and later-field reads are pinned separately.
This does not claim that captured JSON supplies arbitrary executable getters.

A native public caller can supply statuses=[BAD], even though the TypeScript
status parameter excludes it. The new scoped test reproduced the old
`mutation_status` unreachable panic at exit 101. That helper now returns a
visible validation error. All seven declared source statuses retain their old
behavior; the out-of-domain refusal remains a separately priced native API
choice.

Canonical declarations remain the six anchors listed above; this batch adds no
second declaration. Exact-SHA gate receipts and temporary controls live outside
the public repository. No generated corpus or gate output is committed.

### Compiling controls and review closure

Four isolated controls ran at
`20fdd6e1de86c90ca0bd5d49b774df1b01fe9654`. Each compiled, failed at its named
native assertion with exit **101**, restored exact bytes, and passed its named
scoped test with exit **0**. The enumeration stops at native-status panic;
there were no survivors or compiler-only failures.

| Mutation | Observed failure |
| --- | --- |
| Requested-symbol slot routes through unbound_member | WOO-CLASS-RESPONSE native bound slot must bypass unbound host |
| Live membership result is inverted | WOO-MEMBERSHIP native false override must allow known ID |
| Fixture ignores the getter | WOO-CLASS-RESPONSE native getter must throw |
| Native out-of-domain status re-enters unreachable | validated mutation status |

The getter control deliberately breaks the instrument that previously could
false-green; it now fails before the source subprocess. Two sub-agent reviews
cleared the repaired implementation/proof and the two row promotions. Review
also required counting the first slice's requested-symbol behavior defect in
the cumulative discovery tally; the corrected 14/2 split is independent of
residual CHOSEN buckets and of status moves. Neither reviewer ran Cargo.

The final exact-SHA receipt is external. Historical scoped results, this
control SHA and the final workspace gate SHA are distinct measurements.

### Integration formatting follow-up

The fleet directive added `cargo fmt --check` as a fourth required gate.
Integration `7bac54167ca1344621ff98cfc492c2838926bffb` was merged in
`3318aa69441ceee14407c40ca8a434a688fb1a6c`. The lane's response/class/member
sources, tests, oracle scripts, package command and WOO manifests are byte
unchanged from `d78498a481c125a61e7bbb54804e96b8cb3aec0c`; integration's separate
WOO factory/receipt updates remain intact. The exact root wiring assertion is
checked afresh after its clean automatic merge.

The prior wasm helper was deliberately terminated by SIGTERM (measured -15)
when the new directive arrived. It did not pass. Previous scoped/build/host
exits remain attached only to d78498a481; all four required gates are rerun at
the new final head. No formatting mutation was needed after the initial merged
head's fmt check returned zero. The external final receipt identifies the SHA
actually measured and the unrun CI checks.

The final classification review also checked transitive scope against
`sdk-rs/parity/MANIFEST-SPEC.md#woo-public-canonicalizer-bodies`. The two installed
body rows do not impose the old stateless utility boundary: their production
bodies resolve the required import capability. The finite test provider uses
the stateless native utility and does not prove mutable-cache composition in a
concrete native module adapter. That utility CHOSEN row and the owning-class
FINDING remain open; neither body promotion closes those dependencies.

### Algo response audit

This independent eight-row cohort stops at
`decodeWooV3AlgoCancelPendingResponse`; it does not enumerate the remaining
backlog. It follows the preceding 16-unique-row cohort without overlap.
The cumulative discovery split is now **24 unique rows: 21 choice-only and
3 with repaired source-contract defects**. Residual buckets and discovery are
separate measures: this cohort retains eight CHOSEN rows after one repair,
with **0 status moves**. The cumulative two symbol-body promotions remain
2 divergent-to-ported moves. The earlier nine-row signer sample above is a
separate historical audit and is not silently added to this cohort tally.

| Row | Audit result |
| --- | --- |
| WooV3AlgoWriteResponseError | CHOSEN concrete errors, owned mutable gateway data, omitted Error prototype/stack; rationale now explicit |
| decodeSuccessData | CHOSEN captured input/owned wrapper and explicit errors; data-before-timestamp and failure code-before-message retained |
| decodePlaceRow | CHOSEN typed owned mutable row/Result; ID/client/type/decimal validation order retained |
| decodeWooV3AlgoPlaceResponse | Live known-ID method defect repaired; captured descriptor/output/error costs remain CHOSEN |
| decodeMutation | CHOSEN exclusive owned target and captured gateway data; envelope-before-target retained |
| decodeWooV3AlgoAmendResponse | CHOSEN delegated native result/error interface; route label retained |
| decodeWooV3AlgoCancelResponse | CHOSEN delegated native result/error interface; route label retained |
| decodeWooV3AlgoCancelPendingResponse | CHOSEN owned mutable accepted/all-scope/data result; no invented status or ID requirement |

The place decoder formerly used a slice lookup in place of a valid
ReadonlySet's overridden live `has`. The canonical body now accepts a fallible
per-row callback, preserving arbitrary error values by generic Membership
transport; the ordinary slice facade delegates. Unlike regular placement,
all algo rows decode before the first lookup. Each external lookup precedes
internal duplicate detection, and the entire loop precedes the positional
blocker and singleton/type/quantity/client checks.

Native-first assertions inspect decoded enum variants, distinct ID bits,
lexical quantities, timestamp signed zero, ordered lookup IDs, original Rc
error identity, and zero calls after malformed later rows, empty rows or a
wrong request. Nine maintained-source cases stop at a changing `has` getter;
that source case observes lookup receiver and two getter reads independently.
Six additional maintained-source boundary cases stop at pending cancellation,
observing frozen rows/results, shallow gateway copying and nested live aliases.
Native boundary assertions precede that subprocess and inspect concrete error
fields, numeric bits, exclusive target variants and independently mutable maps.
They do not claim captured values reproduce arbitrary executable getter,
prototype, sparse-array, alias or freeze behavior.

The eight annotations now explain why retaining the existing inert typed
numeric/error interfaces avoids an additional runtime-object authority.
The canonical declarations remain the six MANIFEST-SPEC anchors cited above;
this audit adds no duplicate declaration. The existing root WOO oracle command
now includes the algo_writes native target with ordinary failure propagation.
Three leading WOO FINDING rows (signV1, signV3, owning class) remain open.
Exact-SHA scoped, control and required-gate receipts remain outside the repository.

Five compiling controls ran at `95f2401df3` and stop at wrong-error-name.
Each named native test failed at exit **101**, then passed after exact-byte
restoration at exit **0**; there were no survivors or compiler-only failures.

| Control | Named native failure |
| --- | --- |
| Inverted external membership | WOO-ALGO-MEMBERSHIP native false override must allow known ID |
| Internal duplicate check moved before external lookup | WOO-ALGO-MEMBERSHIP native first refusal |
| External lookup interleaved with row decoding | WOO-ALGO-MEMBERSHIP native validation classification |
| Original callback error replaced | WOO-ALGO-MEMBERSHIP native error before positional blocker |
| Concrete native gateway error name replaced | WOO-ALGO-CHOICE native error name |

Two read-only sub-agent reviews found no implementation or classification
blocker. They did not run Cargo. The review boundary remains explicit: no
concrete native Set-provider composition is proved; the callback caller owns
live lookup/receiver dispatch. Native mutation assertions mainly witness outer
mutability/independence, while captured nested ownership is also established by
the inspected native representation. Source nested live aliases are directly
observed. This is a bounded audit, not certification of the remaining rows.

Fetched origin integration `041de6685a5b2c092bfe664057962c917e8b9bd4` was merged
before this unit; a second fetch still named that head. The branch also retains
previously reviewed `8f12062259`, reported merged by the integration owner.
No other unreviewed lane branch was adopted. The controls above and the final
required-gate receipt identify different SHAs; no historical exit transfers silently.


The gate protocol changed during this unit: canonical wasm clippy is now run
centrally, and the lane requires fmt, workspace build and host workspace clippy.
At `549e9532f0`, fmt/build/scoped tests/named wiring passed, but its serial runner
was stopped before the queued wasm launch. The interrupted host clippy measured
-15 and is not counted as a pass. No wasm process was launched. The final three
gates and scoped witnesses are measured again at the delivery SHA after this
protocol note; earlier results remain historical. Source, test, script and
manifest bytes remain unchanged by this documentation follow-up.

### Account request/response audit and lane-wide tally

The running identity list now lives in DIVERGENCE_AUDIT_LEDGER.md. It reconciles
all this lane's divergent-backlog deliveries, including the historical signer
sample, the normalizeSymbol overlap and the partially audited owning-class row.
The earlier limited-cohort counts above are historical, not lane-wide totals.
Neither native BAD-status refusal nor test-instrument repairs inflate the
source-defect count. Inherited audits from the previous WOO owner are not adopted.

This new eight-row batch stops at decodeWooV3SetPositionModeResponse. All eight
retain CHOSEN with explicit reasons for keeping their native consumer interfaces;
there is no newly found dense-data behavior defect and no status move. The
provenance note now correctly says frozen output object rather than input alias.
The set-leverage decoder note also explicitly prices its independently assembled
request/target API domain, broader than the source ready-target literals.

The native-first witness observes provenance fields and mutation, independent
GET query/target maps, SET blocker precedence, gateway fields and signed zero,
exact-key-before-timestamp/code-before-message refusals, reversed hedge-detail
routing with distinct leverage bits, the fixed native FuturesLeverage variant
and the PositionMode variant. Only then does it invoke eight maintained-source
observation groups ending at the position-mode response. Source observations include frozen
objects, GET query-target referent identity, shallow gateway nested alias and
extra source target fields overriding family through object spread. That spread
cost was already declared; it is not newly relabelled as a code defect.

The root WOO command now executes account_leverage and position_mode targets
with normal failure propagation. No runtime route or migration gate changes.
Canonical declarations remain the six MANIFEST-SPEC anchors cited above;
classification rationale is local to these rows, not a second declaration.
This is a finite account-data floor; arbitrary executable getters and complete
mutable native/source object composition remain outside the proof.

Three isolated controls ran at `d52561d1feb15c28f9214be95e5a746c65f578a7`.
All compiled/executed, failed at the named assertion with exit101, restored exact
source bytes, and passed the same named test with exit0. No survivors. The
finite enumeration ends at source-get-alias-copy.

| Control | Observed failure |
| --- | --- |
| Swap native hedge output rows | WOO-ACCOUNT-CHOICE native LONG first |
| Replace native gateway nested payload with zero | WOO-ACCOUNT-CHOICE native gateway nested payload |
| Copy maintained-source GET target into a fresh query | WOO-ACCOUNT-CHOICE source GET target alias |

Review required literal native symbol expectations, direct inspection of the
nested gateway payload and a source error-message assertion. Those proof gaps
are fixed and do not increase the behavior-code discovery tally. Native map
mutation witnesses outer independence/mutability; complete nested alias
composition is not claimed. The source control reaches its named failure only
after all native assertions; the two native controls fail before source launch.

Two read-only sub-agent reviews separately checked behavior/proof and the
inclusive identity ledger/classification. They ran no Cargo. They confirmed
that source constructor dependency reads do not independently certify extra
constructor rows. The partial WOOXExchange row is included because this lane
explicitly audited/updated that owner while installing two bodies; it still
requires the other66 arrows and prototype binding.

Integration parent `1810a76a9764020538782a82b29780b98c9778d1` was fetched and merged
first. The branch also retains the user's accepted, queued algo audit
`e01a0219ff9764d5457007de2346a8b4e007b7ab`; it is not claimed as new work in this
slice. No unreviewed lane branch was adopted. Three fresh required gates apply
to the final delivery SHA recorded externally, separately from this control SHA.
No wasm gate is launched and no push is authorized.


### Error predicates and account validator floor

This eight-row unit stops at requireExactKeys: the two exchange error predicates
and the six account requireRecord/Boolean/Integer/String/Enum/ExactKeys helpers.
Four retain CHOSEN; four change their leading classification from CHOSEN to
MIXED. All remain divergent. There are zero status moves and no new production
behavior repair. The only production edit corrects a stale comment about the
already-landed NaN-minimum guard; its inherited implementation is not credited.
The cumulative ledger now has 49 identities: 39 choice-only, 6 needing code
(three repaired, three partial/open), and four MIXED representation/interface
costs. The binary counter does not exhaust these measured categories.

The native-first witness inspects strict numeric versus string error codes,
short-circuiting of poisoned coercion, falsey fallback versus truthy suppression,
coercion error name, mutable owned record independence, strict booleans, safe
integer bits including negative zero with a NaN minimum, and exact refusal
messages. It observes retained strings, membership/list order and UTF16 key
sorting before invoking either maintained-source supplement. Native assertions
inspect returned values directly, never decode/re-encode equality.

The source observes requireString's returned D800 unit, requireEnum's acceptance
and returned D800 unit, requireRecord's returned one-unit own key and lookup,
and requireExactKeys' equality of that key with the expected entry. Native
JsNumberInput/String ingress rejects the whole scalar with unexpected end of
hex escape; String-key ingress rejects the whole record. The guards cannot run
on those refused inputs. requireExactKeys also cannot carry the unit in its
expected list. Literal six-character backslash-u text and
paired U+10000 have positive native/source observations. These are row-local
witnesses under `sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`, not a new declaration.
The record's retained ownership cost cites
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`; classification uses
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

The captured allowed-list boundary is now explicitly priced: actual source
accepts B against ['A'] when its own includes method returns true, and preserves
the original thrown value from includes/join. Native fixed ['A'] refuses B.
The borrowed-list consumer boundary is intentionally retained for deterministic
margin/position validators; a live-method capability is feasible. Captured
expected-list/object operations and original Error identity are separate
CHOSEN costs. Source getter objects and Error prototypes also remain outside
the pricing JSON snapshot interface: actual Error('RATE LIMIT') classifies true
before serialization, whereas its {} projection and native {} classify false.
This audit found no dense-Value mismatch; it does not claim the mere Value API
violates `sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`.

Two read-only sub-agent reviews identified the four MIXED boundaries and the
previously unpriced includes/join surface before classification. Neither ran
Cargo. The root WOO command retains account_leverage and additionally runs
pricing's native fixture target without failure masking. No full class wiring,
live transport adoption or complete mutable host-object proof is claimed.

The integration merge parent is 53ee502232b005173d8a6f3f0d7cf94bad7f072c;
the accepted, queued account unit 9340c1e6d8bc4ed7241b0313d0565fa831f2bc4d
is an ancestor. No unreviewed lane work is adopted. Required delivery gates are
fmt, locked workspace build and host workspace all-targets clippy, measured
fresh at the final delivery SHA in an external receipt. Wasm is centrally owned
and is not launched; no push is authorized.


Three compiling/executing controls ran against
`ee535ba60fb18396dec8b28bb3bcb0bb9e3a9da8`, each exit101 then restored exit0.
The enumeration stops at source-includes-override-loss. No survivors; restored
tracked source bytes match that commit. The source includes control throws
while evaluating the equality argument, so its observed gate is the named Rust
source-status assertion plus the exact source membership refusal, rather than
the unexecuted inner equality label. The external control harness corrected
that label expectation; it is not a production repair or a surviving mutation.

| Control | Observed named failure |
| --- | --- |
| Native UTF8/byte sort replaces UTF16 key sort | WOO-GUARD native UTF16 key order |
| Source substitutes U+FFFD for returned D800 | WOO-GUARD source returned lone unit |
| Source bypasses own includes override | WOO-GUARD source account-leverage-oracle.cjs; guard must be one of A |

The original development test failed before the surrogate assertion because it
incorrectly assumed shared ingress admitted the value. Correcting that setup is
not counted as a mutation control or a source behavior defect. Native assertions
precede both source controls; the native sort control fails before source launch.


### Position-mode evidence audit

Eight new identities stop at buildWooV3SetPositionModeRequest: two input/result
declarations, fixed account-write metadata, symbol validation, snapshot evidence,
exact decimal zero, zero exposure and the position-mode builder. Five retain
CHOSEN and three become MIXED. A previously counted evidence declaration also
changes C to M after the same programmatic-ID witness, adding no unique row.
The cumulative outcome is 57 identities: 43 choice-only, 6 needing behavior code
(three repaired, three partial/open), and 8 MIXED representation/interface costs.
There is no new behavior repair, no production source edit and zero status moves.
This is a coherent evidence cohort, not a random statistical sample of the venue.

The source returns snapshot reconciliationId as one materialized D800 unit;
the complete typed input retains four identical IDs. Equal IDs reach Ready,
while changing one to D801 selects the mixed-evidence blocker. Rust's required
String fields cause whole shared/declaration ingress refusal: neither the
snapshot helper nor builder executes, and no record/descriptor/blocker escapes.
Paired U+10000 and literal backslash-u strings have positive source/native views.
The four affected rows cite
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`. Ownership costs cite
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`; taxonomy remains
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`, with no duplicate declaration.

Native observations run before source launch: strict symbol refusal, untyped
snapshot null rows and outer independence, exact zero grammar, exposure refusal,
selected declared input members and wider public native construction versus
strict literal ingress, ready enum/method/path/body, absent current-mode fields,
ID-before-margin and exposure-before-pending precedence, whole surrogate-input
refusals, and all eleven independent metadata literals. Mutable local native
result/metadata fields differ from frozen source outputs; no new host wrapper
is adopted. Declaration numeric/schema corpora remain unchanged and still run.

Chosen capability costs are now measured explicitly. Source snapshot rows retain
the original array alias; a push remains visible. Source property getters obey
short-circuit/catch boundaries. Live rows.every can skip a nonzero position or
propagate its original thrown object; sparse-array holes also skip, unlike native
captured [null]. Overriding the source realm's RegExp.test makes string '1' count
as exact zero, while the fixed native predicate refuses it. These capabilities
are feasible but deliberately excluded to keep captured evidence validation
deterministic. No ordinary captured-data algorithm defect was found in this floor.

The result declaration audit is limited to builder-produced operation variants;
it does not prove arbitrary values allowed by the source general request type.
Public native blocker/evidence Strings and nested source/complete fields retain
wider direct construction than TS literal declarations, while native Serde
requires the input source/complete literals. A native data-interface choice is
not relabelled as an algorithm defect or as a language limitation.

Origin aab488a8e2ac3a06ad1f53777831d21659aac289 was merged before this work;
queued guard audit cf9565b226aaf16c1f5fd6d8de27b912f72b9fd3 remains an ancestor.
No unreviewed lane branch was adopted. Two read-only sub-agent reviews separated
capability costs from programmatic string loss and found no dense-data repair.
Required fmt, locked workspace build and host workspace clippy are measured
fresh at the delivery SHA in an external receipt. No wasm or push is authorized.


Three compiling/executing controls ran against
`25e501faf2fa1a7df0729667c4c4a8dfedac0f7a`: every named test exited101 with
the expected assertion, then exited0 after exact source-byte restoration.
The enumeration stops at source-lone-id-comparison-loss. No survivors.

| Control | Observed named failure |
| --- | --- |
| Native Ready body uses ONE_WAY for HEDGE_MODE target | WOO-MODE-CHOICE native ready target body |
| Native zero predicate admits dot-only input | WOO-MODE-CHOICE native rejected zero . |
| Source ignores mismatched ID when account ID starts D800 | WOO-MODE-CHOICE source distinct lone ID blocker |

Both native controls fail before source launch. The source comparison control
passes every native assertion and fails the actual-source D800-versus-D801 branch
observation. Neither serialized output equality nor a source row census can
substitute for those observations. Read-only review tightened snapshot source
expectations from a prefix to all three exact literals and directly inspected
the locally widened native field; those test improvements add no behavior-code
finding. No source/test/manifest changes follow this control SHA, only this
receipt text; delivery gates are rerun at the subsequent documentation commit.


### Algo request and target audit

Eight new function identities stop at buildWooV3AlgoCancelPendingRequest:
requireDecimal, target/common validation, place, target matching, amend, single
cancel and pending cancel. Five remain CHOSEN and three become MIXED. There
are no re-audits, no new behavior repair, no production edit and zero status
moves. The ledger is now 65 unique identities: 48 choice-only, 6 needing code
(three repaired, three partial/open), and 11 MIXED representation/interface costs.
The latest marginal code rate is 0/8; the last two cohorts total 0/16. Cumulative
6/65 is about 9.2%, falling from the front-loaded discoveries. These targeted
coherent cohorts support that observed pattern, not a statistically established
comparison with the full fleet.

Native assertions precede source launch. They pin exact decimal spelling/zero
refusal, exclusive target variants and safe numeric bits, all three hedge close
sides, typed-tag/UTF16-width refusals, independent POST fields across STOP,
TRAILING and POSITIONAL families, exact TP-before-SL child fields, numeric versus
string ID equality, signed zero/Infinity/NaN decisions, target-before-fill-before-
terminal blocker order, unchanged decimal refusal and ready PUT body, single
cancel ID ownership and singular route, and pending cancel's plural route with
no body/query. Owned descriptors remain mutable while source wrappers are frozen.
The existing broader validator/builder/declaration corpora remain unchanged.

Actual source common output and all three place bodies retain algoOrderTag as
one materialized D800 unit for caller inspection. Native shared String ingress
refuses the whole input before either helper runs; no record/request is returned.
Paired U+10000 and literal six-character backslash-u tags have positive output
views in both runtimes. Actual amend compares a stored totalExecutedQuantity
containing D800 against '0' and selects partial-fill before a FILLED-state blocker;
changing only the stored field to '0' selects terminal-state blocking instead.
Required captured/stored native String surfaces refuse the whole input before
any blocker is returned. These three row-local causes cite
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`. The rule is not repeated
here. Owned/frozen costs cite
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`, and classification
uses `sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

Chosen boundaries are explicit: source inherited IDs validate, structural both-ID
targets can match either arm, object tags can pass the source's absent-length
check, and mutable RegExp.test can admit nondecimal strings. The native consumer
interfaces retain own captured fields, exclusive targets, String tags and fixed
grammars deliberately, instead of introducing host property/value/regex machinery.
Normal requireDecimal outputs are ASCII; its audit floor uses fixed ASCII builder
paths. No arbitrary Error-message consumer graph or human-only diagnostic claim
is made. Source single-cancel query is a fresh validated object, not an input
alias. Native None denotes default/undefined/all, not source null. Unknown-family,
non-record trigger and nonboolean current-order refusals stay priced boundaries.
Historical decimalParts repairs are inherited and do not add new code discoveries.

Origin ed8a613e1c09b49c2cf0b3864efef28801c78996 was merged first. The accepted
position-evidence commit a53f2713f2a62f9065433708789dc1067bab9c06 is an ancestor;
no unreviewed lane branch was adopted. Two read-only sub-agent reviews found no
ordinary captured-data algorithm defect and separated unit/tag comparison costs
from unsupported inference about every String input. Required fmt, locked build
and host workspace clippy are measured fresh at the delivery SHA in an external
receipt. No wasm gate or remote push is authorized.


Three compiling/executing controls ran against
`9018323c703c4aa4a34971686ae8e80aa0c64cab`: each named test exited101 at its intended assertion and exited0 after
exact source-byte restoration. The enumeration stops at source-lone-tag-substitution.
No survivors.

| Control | Observed named failure |
| --- | --- |
| Native hedge BUY closes LONG instead of SHORT | WOO-ALGO-REQUEST native hedge closing side |
| Native pending cancel uses singular endpoint | WOO-ALGO-REQUEST native pending plural endpoint |
| Source replaces a materialized D800 common tag with FFFD | WOO-ALGO-REQUEST source common returned lone tag |

Both native controls fail before source launch. The source substitution control
passes all native assertions, then fails the actual returned tag's code-unit
assertion. Read-only review also replaced a fresh-fixture independence check with
the retained input and pinned all three request root types/common fields/routes.
Those are proof repairs, not new source behavior defects. The control source,
tests, scripts and manifests remain byte-identical at the subsequent delivery
commit; the three required gates are measured fresh at that commit.


### Algo guard and metadata audit

Eight previously uncounted divergent identities stop at requirePerpetualSymbol
in woo.v3.algo-writes.ts: the ten-field policy constant and seven basic guards.
Four are CHOSEN and four MIXED; no behavior code or status move is added.
The already-ported hasUnsupportedTrigger is an additional four-literal control,
not a downgraded row or an audited-divergence increment. Read-only review caught
that initial scope mistake before manifest editing.

The ledger now counts 73 unique rows: 52 choice-only, 6 needing behavior code
(three repaired, three partial/open), and 15 MIXED representation/interface rows.
Latest marginal code is 0/8, latest two cohorts 0/16, latest three 0/24.
Cumulative 6/73 is about 8.2%; the discoveries remain front-loaded. This is
three consecutive zero-code cohorts, not yet four, and these targeted cohorts
do not establish a fleet-wide rate or statistical convergence.

Native values precede any source launch or wire comparison. Fixed expectations
inspect all ten policy fields, mutation of its local owned copy without changing
the constant, retained record/array leaf bits and nested values, input independence,
boolean identity and refusals, exact safe-integer endpoints and negative zero,
NaN minimum, nontrimming string validation, allowed-list order, fixed symbol
syntax and all four trigger decisions. The original source corpora are unchanged.

Actual source returns the same record and array, retaining nested aliases,
inherited fields, deferred original getter error and sparse holes. Returned record
kept, array element zero, direct String and an allowed enum String each hold one
materialized D800 unit (typeof string, length one, charCodeAt zero D800), inspected
before projection. Required JsNumberInput/native String surfaces refuse each whole
input before the helper: there is no native map, vector or successful String.
Paired U+10000 and literal six-character backslash-u data have positive returned
views for all four helpers in both runtimes. These row-local costs cite
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`; ownership choices cite
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias` and classification
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

The chosen data interfaces deliberately omit live object/array referents,
Array.includes overrides, mutable Number.isSafeInteger and RegExp.test. Actual
source overrides admit C outside an A-only list, fraction 1.25 and NOT_PERP;
native fixed predicates refuse them. Feasible host capability adapters are not
adopted for these deterministic inert validators. Boolean/integer error observations
use fixed ASCII paths: arbitrary path code units and complete programmatic Error
graphs remain outside this proof, without a human-only diagnostic declaration.
Policy output stays inert; mutable native copies differ from source Object.freeze.

Origin ec5ebe2b90dbad470436cf74040f9af89086f4eb was merged before the unit, onto
accepted queued 3d13f8d008361d25bb1c5286adf2bd484a1ae0b3. No unreviewed lane
ancestor was adopted. Delivery gates are only fmt and locked workspace build;
clippy and wasm are centrally owned and are not launched. Exits are measured
fresh at the delivered SHA in an external receipt; no remote push is authorized.


Three compiling/executing controls ran against
`e5576425859bb2436ea7ae78240931b5ce3bf773`: each named test exited101 at its intended assertion and exited0 after
exact source-byte restoration. The enumeration stops at source-lone-string-substitution.
No survivors.

| Control | Observed named failure |
| --- | --- |
| Native boolean delegate negates its result | WOO-ALGO-GUARD native boolean identity |
| Native policy constant claims runtimeImported true | WOO-ALGO-GUARD native runtime remains inert |
| Source requireString replaces materialized D800 with FFFD | WOO-ALGO-GUARD source string retained lone value |

Both native controls fail before source launch. The source substitution control
passes all native assertions, then fails the actual returned String code-unit
assertion. The retained-input independence assertions inspect their original
captured inputs. Read-only reviews corrected the ported-trigger cohort mistake
and required exact C/A-list and NOT_PERP native refusal counterparts; those
are proof/scope repairs, not source behavior-code discoveries. Control source,
tests, scripts and manifests remain byte-identical at delivery; fmt and locked
workspace build are measured fresh at the subsequent documentation commit.


### Algo input/result declaration audit

Eight new declaration identities stop at WooV3AlgoPlaceResponseRow. One is
CHOSEN and seven MIXED; zero behavior repairs and zero status moves. The ledger
is 81 unique rows: 53 C, 6 code-bearing (three repaired, three partial/open),
and 22 M. Latest marginal is 0/8, latest two cohorts 0/16, and latest four 0/32.
That supplies two further zero-code cohorts after the 65-row receipt. Cumulative
6/81 is about 7.4%; it remains above the cited fleet band. These coherent cohorts
show front-loading, not a statistically established venue-wide convergence.

Native declarations are decoded and every field/variant is inspected before
source execution. Distinct quantities, trigger prices, IDs and TP/SL values
make swaps observable. Owned input mutations retain the actual captured input;
optional absence differs from null refusal. Own undefined in TS is separately
observed before builders omit it. Public family/source Strings remain wider
than checked Serde literals; no new transport or runtime binding is claimed.

Actual builders return materialized D800 tags for STOP, TRAILING and POSITIONAL;
this covers the three input aliases, their union and produced Ready results.
Required native/shared String surfaces refuse each whole input before decoding
or construction. The two other M witnesses are declaration-specific: a virtual
entry imports the maintained trigger/response aliases and uses string-typed
producers/consumers without casts or any. Full pre-emit diagnostics must be zero.
Their returned triggerPrice and quantity are inspected as type string, length1,
unit D800 before projection. A separate in-memory edit changes both maintained
field declarations to number; four independent entry diagnostics must be2322.
No runtime decimal decoder is claimed to admit those declaration values.
Paired U+10000 and literal backslash-u positives are observed separately.

The proof uses the package ESNext/bundler module configuration, strict typing
and the existing skipLibCheck policy, with only the virtual probe as its entry;
imports resolve actual maintained aliases. Initial CommonJS/node10 probe options
failed on configuration diagnostics and were corrected, not suppressed. This is
not a monorepo typecheck. Canonical declarations remain at
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`,
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`, and
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

Origin0286f63437e4320e8c12886d5fef8fc6dfa2eac3 was merged before completing
this unit. The preceding gated local guard unit ccb71519a5 is an ancestor.
Required gates are fresh fmt and locked workspace build at delivery. Host/wasm
clippy, wiring, layers and routing are centrally owned and not run in this unit.
No push is authorized. Exits and mutation controls live in external receipts.


Three compiling/executing controls ran against
`59156a99f90fa9c6aab0cdf38bebea8b02b669b0`: each named test exited101 at the intended assertion and exited0 after
exact source-byte restoration. Enumeration stops at source-declaration-number-quantity.
No survivors.

| Control | Observed named failure |
| --- | --- |
| Native STOP quantity/triggerPrice Serde fields swap | ALGO-WRITES native declaration quantity |
| Native response algo/client ID Serde fields swap | WOO-ALGO-DECL native response algo ID |
| Maintained response quantity declaration becomes number | WOO-ALGO-DECL source maintained aliases typecheck |

Both native controls fail before source launch. The declaration control passes
native observations, then fails the positive compiler diagnostic assertion.
The permanent in-memory two-field checker control separately requires four
entry diagnostics with code2322, before any declaration observation is projected.
Read-only review corrected fixture-only independence checks to inspect the
retained JsNumberInput, without adding a behavior-code discovery. Source/test/
script/manifest bytes are unchanged at delivery; only this receipt text follows
the control 59156a99f90fa9c6aab0cdf38bebea8b02b669b0. Required fmt and locked workspace build run at the next commit.
