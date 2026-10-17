# WOO incremental evidence

Base: core `6922766a6c`; HOW's accepted branch remains at `97dee8b17a`.
This is an in-progress record, not a final venue completion claim.

## Legacy response declarations

Seven source declarations generate 242 cases, checked by one Rust corpus test:
required-key absence/null, optional non-null omission/presence/null, and both
number/string order-ID arms. UTF-8 BOM and NEL remain in strings. The oracle
reads the live TypeScript AST; it describes declarations, not Axios validation.
No test calls a venue. No tests ignored.

- Optional-null control: replacing `T::deserialize(...).map(Some)` with
  `Option::<T>::deserialize(...)` fails at 101 with an assertion naming
  `default_margin_mode.null`; restored afterward.
- CI wiring: removing the WOO step, adding `if: false`, and adding
  `continue-on-error: true` each fail the mandatory-step audit. 48 audit/layer
  tests pass, zero skipped. The named root command uses the existing isolated
  TypeScript install, not an added dependency installation.
- Parity invention/status: pass, 6,901 rows, 963 public items on this base.
  The older denominator is inherited from the requested core base.

Remaining gate controls and venue behavior are not yet claimed.

## Legacy constants

All eleven constants/tables match executed TypeScript, including own-property
entry order and the two environment URLs. Shared enum values are read from the
actual TypeScript enum declarations, without loading unrelated SDK services.
The three typed order tables explicitly disclose their Object.prototype gap;
the live oracle pins five inherited names on each table.

- Staging URL changed to production: Rust assertion failure, exit 101.
- Upstream ORDER_SIDE changed to a null-prototype object: live oracle assertion
  failure, exit 1, `WOO live constants/prototype oracle drift`.
- Restored: three Rust tests pass, zero ignored; plain clippy, scoped fmt, wasm
  build and live constants oracle pass. Extractor normalized the one WOO
  manifest's TOML formatting; no added or orphaned symbols.

## Signing oracle captured before implementation

18 executions of the actual signV1/signV3 source capture the string handed to
create-hmac.update, the resulting signature, signer output, and Axios adapter
input after transformRequest. Axios, qs and create-hmac are the installed source
packages; clock/proxy services are controlled. No HTTP request is sent. The
adapter input is not evidence of endpoint execution or HTTP adapter acceptance.

Missing optional object members disappear from both JSON views; explicit null
remains. Raw JSON strings differ: signV3 JSON-quotes the string while Axios
passes its unquoted contents to the adapter. Falsy non-string bodies sign an
empty suffix and still reach the recording adapter as false/0; this does not
claim the real HTTP adapter would accept those values. Query arrays, empty
queries, key order, proxy URL inclusion, numeric exponent spelling and Unicode
are captured. Signer implementation rows remain pending.

Removing signV1's preimage delimiter fails the live boundary oracle at exit 1
(`WOO signing boundary drift`), then restoration passes. Root CI command pins
all three oracles without a failure mask.

## Core merge-down

Merged `759bd52c8b` (production Store and explicit batch fallback brand) at
`2618a1e5c7`. CI workflow had 55 named steps before and 56 afterward; none were
removed. Both additive conflict areas retain WOO's oracle and the incoming
production model/store seam gate. The combined CI/layer audit passes 51 tests.

## Collation key-set measurement

The signing key set is open. `createAPI` exports an Axios instance with an
unrestricted config.params channel, and `WooExchange.xhr` exposes that instance.
The actual factory interceptor signs a dynamically constructed key map on
`GET /v1/orders`; the recording adapter receives every key after the signer.
`prepareWooV3Request` independently declares a Record<string, QueryValue | ...>
query channel, also passed to localeCompare sorting without an allowlist.

The added factory witness captures this exact preimage:

```
%3F=8&.=9&10=7&2=6&a=3&%C3%A4=4&broker_id=1&brokerId=0&z=5&Z=2|1700000000123
```

The oracle invokes the real WOO factory/interceptor, real Axios and qs, while
replacing the generic factory with axios.create, the retry installer with a
no-op, and clock/proxy services with controlled values. It establishes the WOO
signer's admitted keys, not venue acceptance of those query parameters. These
open keys rule out a closed generated rank table for the exported contract.
No Rust collation implementation or signer parity claim has been introduced.

## Inert V3 contracts and gates

Sixteen declared types are checked with 273 live-source cases; seven constants
retain values and entry order. Every manifest row explicitly states that these
contracts are inert and enable no legacy routing. Independent true-for-false
mutations of PublicReadParity, PrivateReadParity, RegularWriteDemoLifecycle,
AlgoWriteDemoLifecycle, CancelAllAfterDemoLifecycle, AccountWriteDemoLifecycle,
PublicWebsocketParity and PrivateWebsocketParity each compile and fail the
Rust assertion at 101. Restored afterward. No combined mutation is counted.

## WOO journal order-axis projection

29 cases execute the actual TypeScript adapter, using the fixture from the
committed TypeScript test plus optional-field, partial-fill and timestamp cases.
The Rust comparison uses complete typed Execution vectors: numeric values and
array order remain exact; JSON numeric spellings such as 0 versus 0.0 are not
compared. This is a data projection, not signing bytes. It does not claim to
verify shared interval reconstruction or private grading.

Six journal mutations independently compile and fail at 101: executed average
price replaced with limit price; filled quantity replaced with ordered amount;
milliseconds not divided by 1000; fee sign negated; finite zero omitted; and
updatedTime=0 incorrectly falling back to createdTime. Each is restored before
the next. These are projection controls, not claims about the shared fold.

## Accepted-set audit of already-ported unions

All 12 V3 closed string-literal enums accepted externally tagged objects. The
new audit failed at 101 before the fix, reporting 60 object/null witnesses
(one for every variant). Serde-derived wire-name metadata matched the live TS
sets: no pre-existing alias, rename or skip discrepancy was found. All twelve
now require strings before invoking a private remote derive. The remote
definition targets the production enum, retaining compiler-checked variants
and serde metadata; it does not expose a second public deserialization entry.

The live contracts oracle now captures all 12 TS literal sets (including nested
and single-literal fields). Tests compare actual serde metadata, accept every
source literal, and reject tagged objects, arrays, null, booleans, numbers,
unknown/case/whitespace strings and prototype-key strings. The sibling sweep
covers all eight number|string enums across types.rs and v3_contracts.rs:
both declared kinds accepted, other JSON shapes rejected. Legacy account-mode
fields are declared string in TS, not closed unions. No numeric literal version
field exists in these already-ported declarations. Nine package tests pass,
zero ignored; acceptance controls and final gates are recorded next.

The expanded audit found all 27 deserializable records accepted positional
arrays: 15 legacy records (including nested records) and 12 V3 records, including
the two generic wrappers. For each, the regression combines accepted live-source
object fixtures into a complete valid object, then projects its fields in Rust
declaration order into an array. Before repair it reports all 27 as widened;
a missing-field or invalid-value failure is not counted as evidence of strictness.
All records now deserialize a JSON map before invoking a private remote companion.
Serialization and deserialization share the companion's single wire-name list.
The Serialize-only migration descriptor has no decoder to widen. The unlinked,
unclaimed v3_public.rs work-in-progress is not part of this ported-row audit.

The external qualified-call tests exercise the plausible bypass callsite directly:
`Type::deserialize(value)`. A public inherent permissive method would shadow the
strict trait and make these regressions fail. Private companions are inaccessible
to consumers; no public conversion helper or remote=Self decoder was added.

Sequential controls: all 27 map gates independently replaced with the permissive
remote decode, each failing the positional-array assertion at exit 101. A public
inherent PageMeta decoder independently fails the external qualified-call test.
Five enum controls also compile and fail assertions at 101: tagged-object
acceptance, alias, rename, skip, and a public inherent HttpMethod decoder.
These are 33 executions, not 33 newly ported behaviors. Sources restored after
each. The initial enum mutation was formatting-sensitive and failed compilation;
the runner rejected it, the substitution was repaired, and only the subsequent
runtime failure is counted. The item-macro version of the repair also failed the
invention indexer; explicit private companions replaced it rather than exempting
an unindexable public surface.

Restored final gates before utility merge: 11 package tests, 0 ignored, 0 doctests;
package fmt, plain strict clippy --all-targets and wasm32 build pass. All five
live WOO oracles pass. Extraction reports zero changes; invention and status
pass at 6901 rows / 1110 public items; layer checks pass 28 tests. This increment
changes accepted sets of existing declarations, adds no public item and closes
no additional manifest row: WOO remains 491 pending of 534 scoped rows.

## Shared utility merge

Merged feat/rust-p4-utils at 5cf6b4d8d8 after the accepted-set audit push.
Named rust.yml CI steps: WOO parent 56; utility parent 55; merged 56, with
neither parent's names removed. Six conflicted files were resolved per hunk:
additive exemptions (TOML parsed), number formatting delegates to the shared
backend while retaining its signed-byte documentation, additive store sorting
and evidence, facade sort export, and the updated fallible side-normalization
facade test. This makes Number, parseFloat, parseInt, fallible String(any),
number spelling and orderbook sorting available. It does not claim signing
collation is available or close rows whose implementations remain pending.

## Inert V3 public decoding and book/candle mapping

Captured the current TypeScript before implementing this slice. The maintained
oracle now executes 2,044 cases: unknown-input shape/presence/null rejection,
validation order, safe integers and positive timestamps, radix/FEFF/NEL decimal
conversion, extreme/subnormal values, book order/totals, stable candle ties and
millisecond conversion. Number coercion also pins non-callable own toString
throwing, instead of substituting a value or panicking. Six source-derived
record fixtures cover required fields, null/wrong-kind rejection and qualified
array rejection. Explicit field-order lists avoid a sorted-map corpus masking
sequence acceptance.

31 rows are implemented: 30 ported and one CHOSEN divergence. Candle sorting
uses stable merge for the source's nontransitive NaN comparator. Its live TS
witness fixes the observed order [0,1,3,2]; Rust chooses [3,2,0,1]. An upstream
NaN-order correction fails the oracle explicitly. Finite timestamps and ties
match; strict decoders reject NaN, so this affects host-constructed mapper
inputs only. Market and futures-stat mapping remain pending, including the
prototype-property lookup boundary. No runtime V3 gate or route was enabled.

15 independent Rust substitutions compile and assert-fail at 101: radix coercion,
FEFF trim, minimum subnormal, book sort, book totals, candle timestamp unit,
candle order, numeric minimum, kline chronology, five individual record map gates,
and public inherent-decoder bypass. The TypeScript NaN correction independently
fails at assertion exit 1. The flattened book record's private serde visitor
already rejects arrays: its redundant outer map gate is NOT counted as an
independent control. Initial candle-sort deletion failed compilation due to dead
code and was rejected by the runner; substituting reversed order reaches the
assertion instead. Every mutation is restored sequentially.

Final slice replay: all 15 Rust controls and the live TS NaN correction control
failed at assertions, then restored public tests passed. Package tests, plain
strict Clippy and wasm build passed with two Cargo jobs; fmt and all six source
oracles passed. Extraction reports zero changes; invention/status pass at 6901
rows and 1174 public items. WOO now has 70 ported, 4 divergent and 460 pending
rows out of 534. The implementation source has no panic/unwrap/expect/assert
outside test-only code; fallible coercion errors propagate rather than skipping
records. Integration CI remains separate evidence and is not claimed green.

## Lost-const oracle audit

Swept all six oracle scripts. Constants, signing, journal and public behavior
compare executed values; declaration walkers reject unsupported type nodes.
Found one real widening hole: contracts reconstructed every indexed type from
the endpoint runtime table. Removing its const assertion previously exited 0.
The checker now resolves the indexed type and requires literal members. Both
lost-const and broad-annotation substitutions are permanent in-memory assertion
controls; the external lost-const mutation fails with the named closed-set
assertion. Changing the Rust endpoint value also compiles and assertion-fails
at 101, proving the value comparison independently. No row count changes.

## Market mapping

Captured mapper cases before implementation; the public fixture now has 2181
cases, with futures stats deliberately captured but not yet claimed or compared
by Rust. Market filter, active status, IMR ceiling, leverage floor and info
retention each independently compile and assert-fail at 101. Restored tests pass.
Market mapping adds one ported row (71 ported / 4 divergent / 459 pending).
Stats still waits on the shared math_round export. All four existing divergent
WOO notes already carry CORRECTED or CHOSEN buckets; no FORCED claim remains.

## V3 signing boundary

All 12 V3 cases from the original 19 HMAC/Axios captures match preimages,
signatures and prepared configuration. The six V1 cases and exported factory
case remain pending, not counted as exercised. Eight independent controls
assert-fail at 101: raw-string quoting, number spelling, numeric-key order,
method case, proxy URL, indexed query arrays, empty query question mark and
falsy-body signing. The numeric-key test reconstructs insertion order on the
Rust side because the JSON fixture itself has already undergone JS key order.
Signed config and preimage are held together per request; no shared side table.
This closes signV3 as CHOSEN, not a full Axios/adapter parity claim.

## Legacy fills and core ABI merge

Merged core 9fe4b1861f at 7ce5801d3e: named CI steps 56/55 parents, 56
merged, neither name set reduced. One additive exemption conflict was resolved
per hunk and TOML parsed. Core stub cleanup silently removed WOO dependencies;
post-merge compilation caught this and 9bd5264873 restores only used dependencies.
Scoped WOO tests then passed.

Legacy fills has 30 live cases plus source-derived interface presence metadata.
Nine controls reach assertion exit 101: size/start/end names, millisecond unit,
zero-page retention, empty symbol omission, optional-null rejection, map-only
decoding and a public inherent-decoder bypass. The first bypass attempt was a
compile-only unused-import failure and was rejected; retaining explicit trait
and qualified-call checks makes the repaired control reach the assertion.

## Isolated oracle dependencies and integration merge

Integration 7738215fb0 merged at 0ea5edb2bc: named CI steps 56 on WOO,
55 on integration, 56 merged; neither parent's names lost. Five conflict files
resolved per hunk, exemptions parsed as TOML, WOO dependencies retained. Scoped
WOO tests passed; combined layer/wiring audit 181 passed, zero skipped. The
transport tunnel test target compiles against the merged response-bearing ABI.

All eight WOO source oracles pass in a tracked-source mirror under /tmp with no
node_modules in any ancestor. The existing isolated dependency installer now
adds only axios, qs and create-hmac from safe-cex's selectors, resolved by the
copied root lockfile with frozen installation. Signing resolves dependencies
relative to SAFE_CEX_TYPESCRIPT_PATH. Reverting Axios to bare require fails with
MODULE_NOT_FOUND in that mirror; an absent compiler also fails loudly. This is
local hermetic evidence, not a GitHub CI success claim.

History controls: 13 sequential runtime assertion failures (101), individually
covering seconds scale, fractional floor, parseFloat, optional zero, zero PnL,
fee sign, average price, filled quantity, zero updated time, unknown type, open
status, String coercion errors and resolver errors. Corpus: 634 live source cases.

## V3 futures stats

All 2,184 public decoder/mapper source cases are now compared, including the
previously capture-only stats cases. Seven independent mutations fail at runtime
assertions (101): negative-half Math.round, inherited prototype lookup, lookup
precedence, raw-symbol fallback, positive open, required string decoding and
percentage scale. Output absence does not widen the declared input: the external
qualified-call record test still rejects null/missing id and symbol. No V3 route
or migration gate enabled.

Shared primitive consolidation: removed the private signing truthiness, own-key
sort and JSON stringifier after integration supplied their public contracts.
All 12 implemented V3 captures still match preimages, signatures and prepared
configuration. Eight rewritten call-site controls independently fail runtime
assertions at 101; none relies on mutating the shared owner. The V1 collation
and live client factory rows remain pending.

Symbol/hedge utilities: 1,557 live-source cases captured before implementation.
Five sequential runtime controls fail at 101 for prefix handling, reverse suffix,
hedge close direction, Rust-whitespace substitution and colon-tail choice. Two
symbol rows are CHOSEN divergent solely for omitted mutable memoization/cache
surface (with UTF-8 boundary disclosed); the hedge row is ported. No production
panic/unwrap/expect/assert was introduced. All nine source oracles pass in the
no-ancestor-node_modules mirror after adding the utilities oracle.

Correction: WOO's workflow step currently follows the types job's frozen root
install for manifest coverage. The previous bare runtime imports were therefore
not a reproduced CI failure. The isolated selector/resolution change proves the
hermetic contract and removes environment inheritance; it must not be reported
as repairing an observed WOO CI red. The disposable source mirror was deleted
after validation. The separate HOW native CI failure remains a distinct incident.

Isolation review repair: createRequire anchoring still admitted ancestor runtime
packages. Both signing and utility oracles now request absolute package paths
under the configured compiler package's sibling node_modules boundary, retaining
that spelling across symlinks. A permanent CI-wired control removes real qs or
lodash from an isolated install while keeping real ambient copies in its ancestor:
both oracles must fail MODULE_NOT_FOUND naming the isolated path. Complete installs
must pass. Restoring the old createRequire resolver makes this control fail by
name at an assertion (exit 1), because the child unexpectedly succeeds. The
previous no-ancestor mirror proved executability, not absence of ambient fallback.

Pricing/rejection slice: 2,071 cases captured from the actual source declarations
before implementation, including tagged NaN/infinities/signed zero and exact
coercion error name/message. Fourteen sequential controls independently fail
runtime assertions at 101: finite guard, NaN clamp, slippage side, fair priority,
price cap, misses-touch, range fallback, status, primary/alternate rate codes,
mode text, inclusive crossing, fair crossing, and swallowed coercion failure.
Six helpers are ported; two error predicates are CHOSEN JSON-snapshot boundaries.
The live exchange/HTTP dispatch remains pending and is not claimed by these tests.

Response helper slice: 455 live-source cases captured before translation; 451
Rust comparisons and four explicitly counted native-NaN cases outside the chosen
JSON input boundary. These omissions are not ignored tests and not claimed as
parity. Numeric rows are CHOSEN for that boundary, and symbol wrappers inherit
the chosen mutable memoization omission. Ten sequential assertion failures (101)
cover finite funding, blank funding, message padding, both timestamp scales,
unit threshold, zero retention, transaction slash conversion, requested suffix
and candidate order. This claims stateless helpers, not live exchange dispatch.
The new response oracle uses the same absolute runtime loader and is included in
the ambient-lodash absence control and complete-install positive control.

Transaction response-array projection adds 32 source cases (487 total response
cases, 483 compared plus the same four disclosed native-NaN omissions). These
32 were captured alongside implementation, not in the earlier pre-implementation
455-case capture. Nested data.rows wins even when empty, then root rows, nested
data array and root array. Heterogeneous array elements are returned verbatim as
a borrowed slice. Five independent substitutions fail assertions at 101: nested
precedence, empty-array presence, root rows, nested arrays and root arrays.
The transaction row parser remains pending on shared v/vn lookup semantics;
these helpers do not claim live exchange dispatch.

Shared TableHit consolidation removes the stats mapper's local prototype-name
list. All seven stats mutations fail runtime assertions independently at 101.
The initial precedence mutation failed compilation because it left an unused
variable; that was rejected as evidence and replaced by a compiling swap of
both lookup sites. Source cases continue to pin inherited rows with absent
id/symbol rather than silently dropping them.

V1 signing uses the shared pinned root collator with no byte-order fallback.
The original 19 hashing-boundary captures remain intact; four additional V1
cases were captured alongside implementation, not before it. The 23 captures
include 22 signer comparisons and one still-unimplemented factory case. Seven
independent compiling mutations fail runtime assertions at 101: byte sorting,
indexed instead of repeated arrays, treating PATCH as a mutation, ignoring raw
data truthiness, appending an empty query, changing the preimage separator, and
rewriting the caller's method casing. The JSON config boundary is CHOSEN; no
live factory or adapter conversion claim follows from the signer tests.

Section 15 was re-read at integration 35b7ec85f9: whole-record refusal does not
converge with a retained TypeScript record, even after serialization. The WOO
85-case mapper capture separately tags all eleven callable identities and the
prototype object before serialization; it has never claimed convergence.

V1 completion checks: scoped WOO tests 24 passed, 0 ignored; strict workspace
clippy for this crate and all targets passed. All maintained WOO source oracles
and runtime-isolation controls passed. Extractor check added/orphaned zero rows.
Parity initially rejected signV1's missing adapted layer; after correcting that
annotation, invention/status passed at 6,967 rows and 1,316 public Rust items.
WOO plus its journal adapter: 88 ported / 16 divergent / 430 pending of 534.
GitHub run lookup still fails certificate verification (OSStatus -26276); these
are local results, not an integration CI-green claim.

Before client-factory implementation, fourteen additional actual createAPI
interceptor cases capture HMAC update preimages and the real Axios adapter
input (37 total signing fixtures; still only 22 Rust signer comparisons).
Missing/null/string order bodies cause broker assignment to throw and the source
catch returns the unsigned config. Array bodies acquire an own broker_id
property which qs includes; a JSON array cannot carry that extra property.
Remote-signing bypass occurs after broker injection. A v1 substring in the URL
selects V1 signing even on a /v3/ path. Explicit recvWindow zero survives whereas
null defaults to 20000. These are measured factory behavior, not Rust completion
or live endpoint execution; createAPI stays pending.

Scoped wasm32-unknown-unknown cargo check also passed at jobs=2 after the V1
collator integration. This is local target compilation, not hosted CI evidence.

Integration 590f1a4d7e was verified on origin and merged without taking the core
branch separately. Named rust.yml steps: WOO parent 71, integration parent 70,
merged 71; multiset comparison found no missing step from either parent. Two
conflict files were resolved per hunk: the shared primitive audit block had moved
into the crypto-preservation scope, and RestBody exemptions had moved with the
new sign_remote_request entry. Identical moved blocks were retained once. TOML
parsed with 268 unique exemption targets and nonempty reasons. WOO's seven
dependencies remain present. Scoped WOO cargo check passed at jobs=2; CI wiring
and its layer suite passed 174 tests, zero skipped. Section 15 at this integration
head explicitly preserves the whole-record-refusal and prototype-object findings.

Merge lockfile correction: the initial unlocked check resolved 36 packages for
integration's candle-cache native/wasm dependencies, whose manifest was ahead of
its committed lockfile. Restoring that lockfile makes the scoped --locked
--offline check fail at 101 with cannot update lock; retaining Cargo's resolution
makes the same check pass. No existing package/version was removed. Thus the
initial successful compile was not evidence that the committed lock was usable;
the separate lock repair is required for the locked CI build.

Factory composition finding: the candidate now invokes actual shared HttpClient
and classified read retry, not a venue-local HTTP loop. The 14-capture recorder
compared every case before failing: nine discrepancies, eight missing generated
Content-Type headers plus null encoded as literal bytes without JSON content
type. Signatures, broker injection, URLs, base URLs and timeouts agreed. The
exact discrepancy witness is maintained as a FINDING and flips when core changes;
it is not a conformance pass. The candidate stays private/test-linked and createAPI
pending while the shared transform operation is routed to its owner.

Integration d8bcccd413 merged as 4c1ec39af5. The referenced commit is reachable
from origin integration and contains ProtoMember::js_string. Named CI steps:
71 on each parent, 72 merged, neither parent's multiset loses a step. Both lock
parents have identical package/version sets; Git nevertheless duplicated the
identical schannel block. The locked check failed on that duplicate, removed in
a8eb1688ed without changing resolution. The subsequent WOO locked/offline check
passed. Layer/wiring audit: 257 passed, zero skipped. These are scoped local
checks, not an end-to-end CI outcome or wasm Clippy evidence.

Separate wasm-target Clippy was attempted with --locked, --all-targets and
--jobs 2. It failed at 101 in inherited candle-cache IndexedDB code on two
collapsible_if diagnostics, before linting WOO. WOO wasm lint coverage therefore
remains unverified; neither host Clippy nor wasm build stands in for it. Routed
the exact upstream diagnostics instead of editing that owner's implementation.
The live mapper oracle still passes all 85 pre-serialization cases after merge.

Integration 9222ed2454 merged as d794c7912c: 72/73 named parent steps become 74,
none lost. Parsed exemptions: 280 distinct targets, no blank reasons; lock
packages also unique. Layer/wiring: 265 passed, zero skipped. The shared wasm
lint repair exposed one candidate-factory collapsible_if, fixed in 2658fc1030.
The subsequent scoped wasm32-unknown-unknown all-targets strict Clippy passed
with --locked and --jobs 2. This is explicitly wasm lint evidence, separately
from the host lint run.

All seven factory controls produced compiling runtime assertion failures (101):
public bypass, unsigned broker-failure return, remote bypass, array broker
property, version substring, staging broker, zero timeout. The first array
mutation failed compilation by erasing an Option inference constraint; the
runner rejected that result. f1a60284e1 retained the type while removing the
property, and its replay failed the actual capture assertions. Source restored.

Host all-targets strict Clippy also passed separately after the 9222ed2454
merge. Seven V1 controls replayed after the sorted-query extraction, each
compiling and failing runtime assertions at 101: byte sort, array style, PATCH
mutation, falsy data, empty query, preimage separator and method preservation.
No fixture was changed by that replay; source restored after each mutation.

Restored post-merge WOO suite: 26 passed, zero failed, zero ignored across 14
test binaries (all-targets, --include-ignored, --locked, jobs 2). Scoped fmt
check passes. Global parity invention/status passes: 6984 rows, 1402 public
Rust items. The nine-discrepancy factory finding witness passes by reproducing
the open shared defect; it must not be interpreted as factory conformance.

The merged core body-state ABI exposed a candidate-factory adaptation: rebuilding
signed data as Untransformed discarded retry provenance. A real-client recorder
fed a shared-transform-created body containing ` 1 ` under JSON content type;
the old factory compiled then failed the named phase assertion at 101, emitting
`1` instead. The candidate now uses RestBody::with_signed_data to retain the
phase. This is shared-contract composition evidence, not a claim that the open
Axios content-type/null finding is fixed or the factory row is complete.
The phase-preserving candidate passes its three factory tests (zero ignored),
scoped fmt, host all-targets strict Clippy and, separately, wasm all-targets
strict Clippy. No shared owner code or pending-row status changed.

Snapshot declaration audit found mapWooV3Markets was overclaimed as ported:
source info aliases the input instrument, Rust serializes an owned copy. The
live oracle now records both a later alias relabel and the unchanged snapshot;
Rust matches the latter and explicitly differs from the source alias. Changing
the actual TS info initializer to a spread-copy fails the source identity
assertion at exit 1. Three v3_public tests pass, zero ignored. Reclassified only
that row: 87 ported / 17 divergent / 430 pending; no behavior change and no
numeric-loss claim absorbed into the snapshot citation.
Canonical declaration integration 935fc8f578 merged as 30adbd2161 so the new
snapshot citation resolves in this tree. Named steps 74/74 -> 75, none lost.
Real tomllib parse: 284 exemption blocks / 284 distinct targets, no blank
reasons. Locked/offline WOO check passes; layer/wiring 270 passed, zero skipped.
The public live oracle passes 2184 cases plus its explicit alias assertions.

## Factory default Accept coverage correction

All fourteen Axios adapter captures include default Accept. WOO supplies only
cache-control and x-api-key. The earlier recorder compared four selected header
names and did not measure Accept. Adding it exposes fourteen more discrepancies,
for 23 alongside the eight content-type and one null-body discrepancy. This is
an expanded finding witness, not full header-map equality or createAPI closure.
Core reports the default is included in its pending shared repair; replay after
that operation lands is still required.

## Public socket dispatch candidate

126 live cases execute WOO onMessage and BaseWebSocket.recordMessageHandlerStats,
including malformed/falsy/overflow JSON, event-over-topic precedence, all twelve
prototype names, own shadows, empty lists, callback throws, key removal and list
append during iteration. The test-linked Rust candidate uses shared json_parse,
TableHit arity, receive tracking and core live-list dispatch. Scoped Rust test:
1 passed, 0 ignored. Six sequential compiling mutations (disposed guard, receive
tracking, control priority, own shadow, inherited arity, inherited error) each
failed an assertion at 101. Source fixture check and wiring audit (192 passed,
0 skipped) passed. The oracle is appended to the mandatory WOO source command.

This closes no row: inherited core stats are still a routed seam. Nonzero-arity
inherited functions produce source hadHandlers=true/arity before for-of throws;
core sees an absent own list and records false/1. Source stats itself is an
ordinary dictionary: even an own constructor handler mutates an inherited stats
function instead of creating an own entry. The witness retains this ownership
fact separately. No local stats replica or new lifecycle ownership was added.

## Integration 91db928805 merge accounting

Named workflow steps: WOO parent 75, integration parent 76, merged 76; neither
parent loses a named step. Real tomllib parse: 301 exemption blocks, 301 distinct
targets, no blank reasons. All eight WOO dependencies remain. Layer and wiring
audit: 270 passed, zero skipped. The new owned WsParent::options return required
the local recorder to clone its options; this is an ABI adaptation, not a shared
base defect. No stash was used. Full scoped gates and merged-state adversarial
review are still required before merge-up readiness.

## Factory acceptance after shared header repair

The merged core repair now passes all fourteen pre-implementation factory
captures through the actual HttpClient. The comparator checks complete exact
header maps, body bytes, URL, base URL and timeout; the 23-discrepancy fixture is
removed. Public create_api replaces the private test-only candidate and is
claimed CHOSEN for its typed configuration/timeout boundary. No local Axios
header generator was introduced. All seven factory mutations independently
compiled and failed assertions at 101 after this merge.

Source factory options are live aliases: mutating the secret between requests
changes the signature. Rust retains owned initial options. Both views are now
witnessed and the row cites owned-snapshot-vs-live-alias separately from the
other restrictions. A TS snapshot mutation fails the named alias assertion at
exit 1. The original 37 HMAC/Axios fixtures are unchanged.

Adversarial merge reviewer independently verified both parents' named step
bodies/commands, all 48 Cargo manifests, all eight WOO dependencies, exemption
targets/reasons, locks and layer containers. Its sole required follow-up was the
stale factory finding witness and accompanying guidance; the strict acceptance
replacement and updated guidance above address it. Follow-up review and final
gates remain required before readiness is reported.

Factory acceptance positive replay: 30 scoped tests passed, 0 ignored (14 test
binaries plus a zero-test doctest stage). Extractor drift reports 0 updates,
0 added and 0 orphaned symbols. Global parity invention/status gates pass at
6984 rows and 1429 public items. WOO is 87 ported / 18 divergent / 429 pending
of 534; only createAPI changed status in this slice.

## Re-merge integration 6bcff79504

The current-base re-merge retains WOO, Kucoin and GRVT script entries and wiring
assertions. Named steps: 76 / 78 parents to 78 merged, no lost names. Exempt TOML
parses as 303 blocks and 303 distinct targets, zero blank reasons. All eight WOO
dependencies survive. Layer/wiring audit: 278 passed, zero skipped. Host strict
Clippy --all-targets passes on this merged tree; extractor reports zero drift.
The delta adds shared get-key and the two venues; it does not change WOO source.
No stash or side-wide conflict resolution was used; automatic merge was clean.

## Re-merge integration 8cbfb8b5f9

Binance and Bybit landed during validation. One package.json hunk required
union: keep Bybit's script and WOO's longer source-oracle chain. Every other
script from both parents is byte-identical; WOO retains each parent's command
segments plus public-dispatch. Wiring assertions merged without conflict and
were executed: combined layer/wiring audit 287 passed, zero skipped. Named steps
78 / 80 to 80 with neither parent losing a name. Real TOML parse remains 303
blocks / 303 distinct targets and no blank reasons. No whole-side checkout.


### Existing numeric ingress audit (post-8796f510c4)

Audited 92 direct numeric fields across 24 owned records, including seven
untagged instrument numeric unions and four legacy fills-query fields. 1,012
raw/Value/native observations are now named finding assertions; a live Node
JSON.parse witness records bits before serialization. All 92 existing-Value
lexical -0 paths lose the sign. The other 85 preserve raw -0, overflow and
negative underflow, while the seven untagged fields lose raw -0 and reject both
overflow signs and negative underflow. Decimal -0.0 and native Value(-0.0)
retain the sign in all 92. A separate WooV3Rows page-metadata witness pins the
container propagation. No production ingress implementation changed.

Reclassified 23 previously ported source rows to FINDING, including the metadata
container: WOO plus its journal now 64 ported / 41 divergent / 429 pending (534).
Three scoped numeric audit tests pass, zero ignored. Shared repair belongs to
the types owner; no Value-first workaround or local numeric shim was introduced.
The numeric acceptance set in MANIFEST-SPEC now explicitly includes negative
underflow. Separate wasm strict Clippy on 8796f510c4 passed; that precedes these
new test assertions and is not a claim about a later merged tree or CI.

The source-parity expectation control compiled and assertion-failed at exit 101:
`AccountResponse.leverage Value lexical bits`, actual 0 versus source
9223372036854775808. The control changes the expected bits to source parity; it
is evidence of the existing defect, not a production repair mutation. The
maintained finding witness was restored afterwards.


### Integration refresh to 00a1a0069e

Merged current integration after the numeric audit commit. Parents have 80 and
82 named CI steps; merged tree retains 82 with none lost. Real tomllib parse:
312 exemption blocks / 312 distinct targets, no blank reasons and no target
missing from either parent. All eight WOO dependencies remain. Package-script
unions preserve both parents' venue oracle chains. The merged wiring test passes
217 assertions and the layer audit 78, zero skipped. This mechanical record does
not claim the ongoing merged Cargo suite or GitHub CI has passed.


### Resume merge and correction of extractor evidence

RESUME.md was read before continuing. The documented local integration branch
is 6f419e0992; origin still advertised 200f57e95b when fetched. The three commits
between them are verified documentation commits. Merge target is the documented
local head, without changing the integration branch or its remote ref.

Parents have 82 and 85 named CI steps; merged tree retains 85, none lost.
Real tomllib parsing gives 312 exemption blocks / 312 distinct targets, no blank
reasons and no target lost from either parent. Eight WOO dependencies remain.
The spec conflict was resolved within the numeric-ingress section: integration's
canonical section/anchor and subsequent sections remain, with WOO's measured
negative-underflow acceptance detail incorporated under its acceptance set.

Correction: earlier extractor-green reports described local executions, not a
clean-install gate. The resume run again reports zero drift, but resolves
smol-toml from this worktree's existing node_modules/smol-toml/dist/index.cjs.
The root lock/workspace wiring defect documented in RESUME.md remains a shared
gate-owner issue; this run does not close it. No dependency was installed to make
the gate green. The registry example failure described there is also not claimed
fixed by WOO. GitHub access remains unavailable; no CI-green claim is made.


Resume taxonomy normalization: nine owned divergent notes had their existing
bucket after introductory prose. Moved that bucket to the beginning as section
18 requires; no bucket, witness, status or behavior changed. Scope remains
64 ported / 41 divergent / 429 pending of 534 including the journal adapter.


### Resume merge-up validation result

Merge 5735df7dd1 plus note-only normalization 57ef40ef5a passed the full scoped
WOO suite: 33 tests, zero ignored, and the zero-test doctest stage completed.
The complete source-oracle chain passed. Strict host Clippy and strict
wasm32-unknown-unknown Clippy ran separately with all targets, jobs 2, locked and
offline; both passed. Layer/CI wiring ran 305 checks, zero skipped. Global parity
passed at 7,334 rows / 2,413 public Rust items. Extractor local drift was zero,
with the existing-node_modules limitation above explicitly retained.

Independent read-only merged-state review returned MERGE at 57ef40ef5a: no
blocking WOO merge-integrity or misleading-audit findings. It verified full CI
step command bodies/multisets (82/85 to 85), both parent script/assertion sets,
312 exemption blocks / 312 distinct targets without lost or changed reasons,
all 49 Cargo manifests, eight WOO dependencies, layer containers, lockfiles,
the spec union, 92 fields/24 records/1,012 observations plus nested Rows, all
23 FINDING reclassifications, and the nine note-only bucket moves. It inspected
the test/control logs; it did not rerun builds or claim CI success.

Measured scope: 64 ported / 41 divergent / 429 pending of 534. The factory is
closed. Pending shared dependencies remain inherited-handler statistics and
try_parse_json finite normalization; the declared numeric ingress findings
await the shared ingress repair. Clean-install extractor wiring and the
inherited registry example failure remain shared base issues, not closed by
this venue merge. No GitHub CI result is claimed.

### Legacy limit/algo mapper slice

Source captures landed first at 8742727ad8. The implementation uses the real
Store trait, shared get-key, TableHit and JS coercion, without a local prototype
resolver. The 103 source cases distinguish 25 matching projections, 77 declared
whole-record refusals and one matching source TypeError. Eleven callable
prototype members are tagged before serialization separately from __proto__.
Both rows are MIXED: closed output refusal, the separate prototype-object
FINDING, dynamic JSON-to-typed output restrictions, and owned parent info are
stated independently. No serialized-convergence claim is made. Runtime exchange
registration remains pending on its own rows.

Four mapper tests pass, including ordered children, parent info/IDs, clock calls,
and 180 pre-serialization native field observations from 36 actual-source
numeric cases. The numeric corpus includes lexical -0, -0.0, negative underflow,
both overflow signs and binary64 rounding. It does not pass through a typed
record rebuffer and does not close the existing 23 ingress FINDINGs.

Nine sequential compiling mutations fail at named assertions (exit 101):
truthy fallback, algo filter, trigger branch, parent info, parent ID, clock,
inherited identity, finite signed zero, and overflow. The runner rejects
compile-only failure and generic unwrap panic as proof. The source null-prototype
and snapshot-info mutations fail at their named assertions (exit 1); --check
runs both children and requires those failures. Replacing the snapshot mutation
with a no-op fails the parent child-exit assertion, proving execution alone is
insufficient. The order oracle was appended to the root WOO CI command and its
exact wiring assertion. The full source-oracle command passes; layer/wiring
reports 227 passed, zero skipped. Host and wasm strict Clippy pass separately
with all targets, locked/offline and jobs 2. Local extractor drift remains zero,
with the previously documented existing-install limitation. Exemptions remain
312 blocks / 312 distinct from tomllib, zero blank reasons.

Final restored-code scoped suite: 37 passed, zero failed/ignored, across 17
result stages including doctests. Parity caught the missing orders.rs file
registration and incomplete exact-target rename notes; both were corrected.
The rerun passes at 7,334 rows / 2,415 public Rust items. Scope now measures
64 ported / 43 divergent / 427 pending of 534. This is an incremental mapper
slice, not a new merged-state review or a claim that the pending runtime is
implemented. No GitHub CI outcome is available.

### Historical asset overflow repair

An owned local truthiness helper still used Number::as_f64, dropping a numeric
1e400 fee/rebate asset where actual TypeScript takes the truthy branch and emits
the open string Infinity. The new named witness failed on the old code at
fee_asset 1e400 (None versus Some("Infinity"), exit 101). Removed that helper
and delegated to shared js_truthy, retaining shared fallible String. Fourteen
live-source cases inspect the resulting asset strings before serialization,
covering both fields, both overflow signs, signed zero, underflow and ordinary
numbers. All four history tests pass after the fix; all fourteen sequential
history controls compile and assertion-fail independently, including the new
asset-overflow mutation. The existing 634-case history oracle also stays green.
No rows change status: 64/43/427. This narrow fix is checked by scoped history
tests; the next integration merge gate will run host and wasm lint on it.

## Numeric ingress adoption on c659b17b74

Merge `78d7493d38` retained all named CI steps: 85 / 87 parents to 87,
with 323 exemption blocks / 323 distinct targets from tomllib, all eight WOO
dependencies and 237 passing merged wiring/layer assertions. This is local
merge evidence, not a remote CI or clean-install extractor claim.

Thirty-three record constructors now capture shared JsNumberInput before their
private remote companions, retaining shared strict_decode::object. All eight
number|string unions do the same at their standalone public Deserialize entry.
Explicit exhaustive JsNumberOutput registrations check their output graphs.
Generic response/rows payloads require a registered output type; the tests use
JsNumberInput instead of Value. This is a public-decoder adoption, not merely
registration or an opt-in helper that leaves bare serde unchanged.

The old finding assertions first flipped at AccountResponse.leverage and nested
Rows metadata. The replacement corpus passes 91 eligible fields through seven
routes: bare raw string, raw slice, owned Value, borrowed Value, and shared
raw/owned/borrowed entry functions. Native Value(-0.0) is a separate guard.
It covers -0, -0.0, positive/negative overflow and negative underflow and compares
bits before serialization. Eight standalone unions pass six routes including
Type::deserialize; Rows metadata passes six routes. The live Node bit witness
still runs. The remaining snapshot finding retains its original raw/Value and
native-Value guard until the shared OrderBook registration is merged here.

All five numeric controls compile and fail at named runtime assertions: four
restored Value map buffers lose signed zero, and a standalone untagged decoder
bypass refuses raw -0. The initial union control produced a generic unwrap
panic; that was not accepted as evidence, and the permanent test now asserts
input acceptance by type, path and spelling before reading its bits. Thirty-three
individual map-gate removals plus a public inherent-decoder bypass independently
fail positional-array assertions (34 controls). The runner asserts the exact
record inventory so a stale source pattern cannot silently execute zero cases.

The scoped suite passed 40 tests, zero ignored; the expanded five numeric tests
then passed after adding the bare borrowed/slice and public-qualified routes.
Twenty-two numeric FINDING rows return to ported: 86 ported / 21 divergent /
427 pending of 534. WooV3MappedOrderBookSnapshot remains a FINDING on this base.
Host/wasm lint and final merged-state gates will run after consuming the newly
landed shared census; this paragraph does not claim those gates have run.


## Final numeric snapshot adoption after shared census

Merged the shared output census at 342e3c1165 (82445b433d), then the reviewed
try_parse_json repair at 0067c8c6f5 (6cb9d48f70). Named CI steps were 87/87 to
87 and 87/86 to 87 respectively. A real tomllib parse reports 327 exemption
blocks / 327 distinct targets, with no duplicate or blank reason and both
parents' target sets retained. All eight WOO dependencies survive. The census
merge passed 237 wiring/layer assertions and scoped WOO compilation; the parser
merge passed four focused transport tests and WOO compilation.

WooV3MappedOrderBookSnapshot now uses shared numeric capture inside its public
Deserialize and the landed foreign OrderBook registrations. Five numeric tests
pass: 92 record fields across 24 records, all eight standalone unions, nested
Rows metadata, and a separate flattened-book witness for timestamp plus six
bid/ask fields. Bare raw/slice/owned/borrowed and shared raw/owned/borrowed paths
compare native bits before serialization for -0, -0.0, both overflow signs and
negative underflow. The source Node bit oracle remains active.

Restoring the snapshot's Value map buffer compiles and fails the named source-bit
assertion at 101. Removing its outer map guard survives because the private
flattened companion independently rejects sequences; this is a MASKED control,
with no independent coverage of the outer guard, and that mutation is NOT
counted as a failing control. A public inherent positional adapter then compiles
and fails the qualified-call positional-array assertion at 101. Together with
the preceding adoption's 39 controls, 41 compiling controls have independently
failed runtime assertions. This final row returns to ported: 87 ported /
20 divergent / 427 pending of 534. Final merged-tree suite and separate host/wasm
lint are still to run; no CI result is claimed here.

## Store notification sweep

Actual TypeScript AST inventory found 17 update-family calls across 11 WOO
symbols, all still pending: refreshFundingRates, applyOptimisticOneWayPositionFromFill,
start, tick, confirmPositionModeAndRefreshPositions, cancelAlgoOrder, private
handleExecutionReport/handleAlgoExecutionReport, and public ticker/BBO/mark handlers.
No setSetting call exists in WOO. Rust WOO has no Store writes; its only Store
access is markets_by_symbol in already-divergent order mappers. Therefore zero
already-ported rows cross the routed fallible-notification seam and no row was
reclassified by this sweep. This does not clear pending writers.


## Core consolidation and final scoped gates

Merged aa632b5578 at 8fe451d3cc after the integration freeze was lifted. Both
parents contain 87 named CI steps and all 87 remain. The real TOML parse reports
327 exemption blocks / 327 distinct targets, no blanks or duplicates, and all
parent targets retained. WOO retains eight dependencies. Merged wiring passes
237 assertions; the independent layer audit passes 78.

The new fallible record_message_handler_stats contract required WOO's test-linked
public dispatcher to propagate its Result before callback iteration. The prior
read-seam finding witness flips: core recognizes inherited handler arity and
refuses global inherited stats writes rather than creating an incorrect own
entry. The remaining witness explicitly records that write boundary, including
an own constructor handler whose stats key still inherits. The runtime row is
still pending; consuming a core read repair does not register a venue socket.

On this merged tree: the full WOO suite passes 40 tests, zero ignored, plus
zero doctests. Host strict Clippy with all-targets passes. Separate wasm32-unknown-unknown
strict Clippy with all-targets also passes, without no-deps. All builds/tests
use locked, offline and jobs 2. WOO fmt check passes. All mandatory WOO live
oracles pass, including complete factory headers/data and runtime dependency
isolation. Parity passes 7334 rows / 2538 public items. Extractor drift reports
zero manifest updates, additions or orphaned symbols, using the existing
worktree installation; this is not clean-install extractor wiring evidence.
No GitHub CI outcome is claimed.

The notification sweep is extended from direct calls to internal reachability
in STORE_NOTIFICATION_AUDIT.md: all 42 ported callable bodies resolve, and
transitive calls terminate without a notifying core operation. Zero affected
ported rows remains the result, with caller-supplied effects explicitly outside
that fixed internal graph. Counts remain 87 ported / 20 divergent / 427 pending.
Merged-state adversarial review is still required before merge-up readiness.


## Incremental consolidation after the full review

The adversarial merged-state review returned MERGE for implementation/integration
at 9730fab104, requiring one pending-note correction. That note is narrowed in
6aa3904428: core inherited-handler reads are repaired; global inherited stats
writes remain distinct. No runtime row was reclassified by the correction.

6aa3904428 merges OKX integration eefa54442a. Both parent step multisets survive:
87/88 to 88, including the incoming cancellation gate. Merged wiring passes 241
assertions. Scoped WOO wasm all-targets strict Clippy passes again after the
incoming candle-cache wasm change. This is lint evidence, not browser execution.

cd83d7538f merges P6 integration 1f6698344c. All 36 incoming files were compared
byte-for-byte with that integration head and match exactly. WOO source, tests,
fixtures and manifests are unchanged from the preceding reviewed/adapted tree;
workflow, package scripts, wiring test, layer policy, workspace Cargo manifest,
Cargo.lock and WOO dependencies are unchanged by this second merge. Both
parents have 88 named steps and the result retains them all. Real tomllib parse:
333 exemption blocks / 333 distinct targets, no blank reasons, no duplicate,
no target removed from the preceding parent (six added). WOO still has eight
dependencies. Scoped locked/offline jobs-2 WOO check passes. Parity rerun passes
7334 rows / 2548 public Rust items. The earlier full WOO suite, source oracles,
host/wasm lint and extractor evidence are cited with their original scope;
they were not rerun wholesale for this unchanged venue delta.

Counts remain 87 ported / 20 divergent / 427 pending. An incremental adversarial
review of these two merges is requested in addition to the full verdict above.


The incremental reviewer extended MERGE to c8477f0188, independently confirming
both incoming-file identity and unchanged venue/gate inputs. Integration moved
again before readiness: 16debf4f8c consumes b0e0b9c68b (28 incoming files exactly
matching integration), and 6798c2bbca consumes 7772471f39 (one exact spec-only
file). WOO source, tests, fixtures and manifests remain unchanged; workflow,
package scripts, wiring, layers and WOO dependencies remain unchanged. The
container-eligibility merge adds only existing-package dependency edges to the
lock: indexmap for num and num for GRVT, with no version change. Scoped locked,
offline, jobs-2 WOO compilation passes after that dependency delta. Final parity
passes 7334 rows / 2559 public items. Both parent step counts remain 88 and all
steps survive; exemption parsing remains 333 blocks / 333 distinct targets.

Read the new inherited-write spec note: a prototype hit can become an update to
a different real record whose id is the string undefined. This is separate from
a value leak and must be witnessed at the Store write when WOO ticker handlers
are implemented. Current WOO ported callables still have no notifying write path.
The prior full and incremental reviews are retained with their scopes; the last
two merges require their own short review extension before readiness.


## Public heartbeat operations

Source-first commit 4fa673787b captures 40 actual ping/handlePingEvent cases
through the actual base.send implementation, before the Rust implementation.
The public operations retain disposed/readiness guards, exact ping/pong frames
and debug logs, host errors, timestamp-before-send ordering and zero reconnects.
Five independent compiling mutations fail named runtime assertions at exit 101:
disposed-clock, missing-clock, clock-after-send, swallowed-send and pong-payload.
The restored scoped run passes 7 library tests plus the heartbeat test, zero
ignored; this also checks the modified socket fixture against existing dispatch.
Live oracle check passes 126 dispatch and 40 heartbeat cases. Host strict Clippy
and separate wasm32-unknown-unknown strict Clippy both pass, all targets, jobs 2,
locked/offline, without --no-deps. WOO fmt and diff whitespace checks pass.
Parity passes 7334 rows / 2561 public items after adding exact extracted-target
rename notes; the first parity attempt correctly rejected the incomplete notes.
Measured WOO count: 89 ported / 20 divergent / 425 pending. Socket lifecycle,
registration and data writers remain pending. No full-suite replay, CI-green,
browser-wasm or clean-install extractor claim is made for this slice.
The callee-based Store mutation-result audit remains zero; see its inventory.


## Public ticker, BBO and mark writers

Source-first 5741bd9970 captures 87 actual handler/production DefaultStore cases,
including constructor/population, first-patch commit, subscriber throw and batch
abort, and numbers observed as bits before serialization. The three extracted
operations use native ModelPatch numeric values and drop the lookup borrow
before writing. Both the mutation result and error conversion name
StorePatchError explicitly. Notification identity remains a routed FINDING;
copied dynamic values refused by typed numeric fields remain CHOSEN divergences.
Six compiling mutations independently fail named runtime assertions at 101:
ticker-field, bbo-field, mark-field, spot-filter, truncate-batch and zero-sign.
Restored scoped tests pass 8 library tests and 1 heartbeat test, zero ignored.
Live oracle checks pass 126 dispatch + 40 heartbeat + 87 writer cases.
No socket registration is claimed. Counts: 89 ported / 23 divergent / 422 pending.

Integration refresh d898abe6e4 consumes 18261b604c. Both parent step multisets
(88 and 90) survive in the 90-step merge; real TOML parse reports 333 exemption
blocks / 333 distinct, no parent targets lost. All eight WOO dependencies remain.
Layer and wiring tests pass 329. The candidate import correction was committed
separately at 8a019cb9b0. This is scoped evidence, not merged-state review or
full-suite/CI/browser-wasm/clean-install-extractor readiness. Separate lint and
parity checks follow the requested domain-model integration refresh.


## Refusal timing follow-through and domain-model refresh

The first writer witness only asserted Err on a missing-field row followed by a
null symbol. That was insufficient: it accepted the earlier typed refusal in
place of the later source TypeError. The implementation now defers typed refusal
to batch end; the three existing cases compare exact source name/message. The
new eager-refusal mutation compiles then fails that assertion at 101. Its first
form failed compilation on an unused label and is NOT counted; the corrected
form preserves a live label use and fails at runtime. The previous six controls
retain their earlier-tree scope; this seventh control measures the timing repair.

The adjacent algo mapper has a separate measured FINDING: source filters the
whole child array before mapping, while Rust interleaves them and refuses eagerly.
An inherited first child then null gives a retained source order and one clock
call; then numeric 1 gives a source in-operator TypeError and zero clock calls.
Rust returns the earlier inherited-type refusal in both cases. The live oracle
and orders test pin exact errors and clocks in algo-batch-priority.json. The
already-divergent row is corrected without changing its status; this finding
is not claimed repaired by the public-writer change.

Restored scoped run on the domain-model refresh passes 14 tests (8 library,
5 order mapping, 1 heartbeat), zero ignored. Live order oracle passes its 103
existing cases plus the two new priority captures and source mutation controls.
Merge 4d188ae6b0 consumes e92bdd9b04: all 38 incoming files exactly match that
head; CI steps 90/90 to 90, eight WOO dependencies, real TOML parse 338 blocks /
338 distinct and no parent targets lost. Layer/wiring tests pass 329. Host
strict lint passed during the writer priority repair; final separate target
checks and parity are recorded below once they finish. No merge-up readiness
or full-suite replay is claimed here.

Final checks on this tree: separate host and wasm32-unknown-unknown strict
Clippy both pass with all targets, dependencies included, locked/offline jobs 1.
Parity passes 7334 rows / 3216 public items. Extractor reports zero changed
manifests, zero added and zero orphaned symbols using the existing worktree
installation; this is not a clean-install wiring claim. WOO counts remain
89 ported / 23 divergent / 422 pending. The six original writer controls plus
the separate compiling eager-refusal control are recorded with their scopes
above; the restored 14-test run covers their final production implementation.

## Private heartbeat and timestamp helper

Source-first commit 97a98976cf contains 40 actual private ping/sendWithLog cases
and 125 actual parseTimestampToSeconds cases. The Rust candidate then passed
11 scoped tests (8 library, 2 private socket, 1 public heartbeat), zero ignored.
The timestamp comparison uses bits and exact TypeError name/message; its single
native-NaN case is explicitly outside the selected JSON input contract. The
source witness still pins its undefined result. No socket registration is claimed.

Seven sequential compiling controls fail at named runtime assertions, exit 101:
disposal guard, timestamp after send, diagnostic wrapper bypass, swallowed send
error, unit threshold, flooring, and zero acceptance. The first disposal-control
runner expected the timestamp assertion, but the diagnostic-count assertion
fired earlier; this was a real rejection, not a surviving mutation. The runner
was corrected to name that earlier observable and all seven replayed. Source
restoration is in finally. Diagnostic-check counting distinguishes sendWithLog
from send even when their wire frames match.

The existing live socket oracle gate now includes both private captures; its
126 dispatch, 40 public heartbeat and 87 Store writer cases also remain green.
Notification follow-through terminates at heartbeat classification before
reconnect/health publication; no new Store writer or inferred conversion exists.
Counts: 90 ported / 24 divergent / 420 pending. Final restored tests, separate
host/wasm lint and parity results are recorded below after completion.

Restored scoped run passes all 11 tests, zero ignored. Host strict Clippy and
wasm32-unknown-unknown strict Clippy pass separately, all targets with dependencies
included, locked/offline jobs 1. Parity passes 7334 rows / 3218 public items.
Extractor drift reports zero changed manifests, zero added and zero orphaned
symbols with the existing worktree install. No CI-green, browser-wasm, clean
install or merged-state review readiness is claimed.

## Interface heritage inventory guard

A sweep of WOO scripts found one InterfaceDeclaration.members inventory, in
fills-oracle.cjs. It previously ignored heritageClauses. The current source has
no inheritance, so existing row behavior/counts are unchanged. The inventory now
fails closed before reading members. A permanent source mutation adds
ExtraNumeric.hiddenNumeric:number through extends and requires the named refusal.
The ordinary 30-case live fills oracle and schema freshness pass. Removing the
guard in memory makes the permanent control fail at its named missing-exception
assertion, proving that unchanged local fields cannot satisfy it. Type literal
schema walks and class method extraction were inventoried separately; neither
is an InterfaceDeclaration member census. No Rust suite replay is claimed for
this independent oracle-only repair.

## Algo filter phase and enum-output refusal timing

Source-first 3379f31f01 captures 12 actual mapper cases before the repair. The
source distinguishes two clocks (inherited then ordinary), one (inherited then
ID/side coercion error), and zero (ID coercion error before an inherited trigger
output refusal). Null and JSON-array filter children disappear; six primitive
RHS cases throw before all mapping. The candidate passes six scoped order tests
and the live 103 ordinary/lookup cases, 36 numeric cases, two original priority
cases and 12 expanded timing cases.

Whole-array filtering is now separate from mapping, retaining O(n) borrowed
child references just as the source allocates its filtered array. Enum-output
refusals wait until the batch ends; later source coercion errors and clock reads
are retained. No successful subset returns. This repairs only closed enum
refusal continuation: dynamic numeric/bool typed refusals still occur eagerly
and retain a separate FINDING for possible error/clock masking. Primitive RHS
error text remains a separately routed shared get-key FINDING; no local
in-operator replica was introduced. Row status/counts do not change.

Five sequential compiling controls return 101 at the intended runtime failures:
early type refusal (clock count), early trigger refusal (later source error),
stop after first refusal (clock continuation), clock during filter (phase), and
successful subset (the named algo-refusal test's required Err). Restoring source
is guaranteed by the runner's finally clause.

The prior /tmp isolated compiler installation disappeared during the interruption;
its attempted oracle rerun failed MODULE_NOT_FOUND, not a skip. The rerun with
SAFE_CEX_TYPESCRIPT_PATH pointing to this worktree's actual node_modules/typescript
passes all source freshness and correction controls. This is existing-install
evidence, not a recreated isolated-install validation. Real tomllib parsing via
Homebrew Python reports 90 ported / 24 divergent / 420 pending, unchanged.

Final restored order suite: six passed, zero ignored. Host and wasm32-unknown-
unknown strict Clippy pass separately with all targets/dependencies, jobs 1.
Parity passes 7334 rows / 3218 public items; extractor drift is zero changed,
added or orphaned. No CI or merged-state readiness claim.

## Algo primitive filter adoption

On integration merge `b56689bf91`, the mapper uses shared `v_unknown` for the
whole child filter. The prior primitive-RHS message FINDING is repaired; dynamic
numeric/bool refusals, closed outputs and owned info remain separate divergences.
The unchanged actual-source capture checks 103 mapper and 12 timing cases.
The restored `orders` target passes 6/6, with 2/1/0 clock counts retained.

All six `algo-timing-controls.py` mutations compile and exit 101 at their named
assertions. The new generic primitive-error mutation fails `ALGO-FILTER primitive
RHS exact source message`; early type, early trigger, stop-after-refusal,
filter-clock and successful-subset controls were replayed, not merely inherited.
Fixture bytes are unchanged. This bounded slice makes no whole-venue cardinality,
CI, merged-state readiness or fresh host/wasm Clippy claim. Counts remain
91 ported / 33 divergent / 410 pending across 534 rows including journal.

## Transaction parser: three distinct evidence updates

Iteration 1 executed 265 actual-source/native cases; iteration 2 executed 269
with four supplements. These historical passes did not cover a final-null fee
alias. Iteration 3 adds final `total_fee: null`, final absence and final
`lastPrice: null`, reaching 272 cases. The final-null-to-absence production
mutation compiles and exits 101 at `TRANSACTION native commission`; the gap is
now closed for those operands. The restored target checks 13 errors, 86 rejected
rows and 173 returned records, with 2249 direct native field comparisons.

The timestamp-overflow premise was incorrect and was removed before committing
classification: values >=1e12 use the millisecond branch, while only smaller
values are multiplied by 1000. The proposed finite-output rejection mutation
was never executed and supplies no evidence. The replacement unit mutation
fails `TRANSACTION native timestampSeconds`. Identical ID/maker coercion errors
are retained as a multiple-error case, not a proof of which error happened first.

Seven production controls compile and fail named assertions: price/quantity,
order/fill IDs, maker NaN, truthy-vs-nullish fallback, eager price rejection,
timestamp units and final-null retention. An initial runner expected the
truthy mutation to hit rejection; it instead correctly hit orderId identity.
The expectation was corrected and all seven were replayed, then restored.
The live-source asset witness preserves input/result alias identity and later
mutation; Rust retains its owned snapshot, a separately declared choice.

This extracted mapper adds no exchange runtime registration. Native NaN or
executable input hooks are outside its JSON boundary; finite fields retain bits
before serialization. Source capture reports architecture and Node major and
canonicalizes only NaN payloads. Pinned PR 7274 is unchanged; this work is local.

Final scoped checks for this slice: restored transaction target 1/1 (272 cases),
host Clippy `--lib --test transaction` exit 0, separate wasm Clippy `--lib` exit 0,
changed-file rustfmt and diff checks exit 0. Real Yarn extractor check exits 0.
Parity exits 1 with 19 diagnostics, all in AsterDEX files unchanged from merge
`b56689bf91`; no WOO diagnostic. This is not a green parity or merge-up sheet.
An accidentally overlapping parity invocation was stopped by captured own PID
after verifying its cwd, then rerun serially; the stopped run has no verdict.
Source-tree enumeration including WOO journal gives 91 ported / 34 divergent /
409 pending of 534. The parser row moves pending to divergent for the explicit
representation and ownership boundaries, not to ported.

## Legacy history fetch: source-only preparation

`legacy-history-oracle.cjs` executes the actual fetch method, candidate generator,
response selector, parameter builder and endpoint declaration. Its fake XHR
measures method control flow and request arguments, not Axios wire behavior.
The arm64 Node 24 capture pins 90 cases / 278 calls and a fixed per-case attempt
inventory. Source-only mutations remove the unqualified attempt, swallow a
retained error and continue after success; all execute and fail their named
assertions. No Rust implementation, compiling control or row closure is claimed.

Empty successful responses retain an earlier error; falsy rejected values replace
it and suppress the terminal throw. Null response destructuring differs from
null data. Mixed arrays retain their members and identity. Options mutated after
the first request are reread on the next attempt; initial and final views are
captured separately. Output is generated only under /tmp, never committed.

## Legacy history fetch: RestAdapter implementation

The callable operation preserves candidate order, the final unqualified GET,
nonempty short-circuit and the last RestFailure across empty successes. It uses
shared RetainedJsonInput for native query numbers. The source-first corpus stays
at 90 cases / 278 calls; 54 cases compare against Rust, while 36 named cases
exercise boundaries the typed adapter cannot represent (falsy rejection, null
response envelope, mutable options). The comparable subset independently pins
158 native requests, 54 returned rows and 20 errors. This is not Axios wire or
response-transform evidence; byte-only response provenance remains a FINDING.

Five production controls compile and exit 101: fallback order at `LEGACY native
request sequence`, swallowing at `LEGACY expected retained error`, and empty,
subset and extra output at `LEGACY native row cardinality`. Restored target is
2/2. Sixteen separate native observations cover each size/start_t/end_t/page with
signed zero, both infinities and shared NaN refusal before any send. These native
parameter checks are separate from the finite actual-source corpus.

The first native run failed because serde Number spelling compared 37.0 against
37. Numeric observations now compare binary64 bits, retaining signed zero;
whole request and row structures still compare all keys and values. After the
five controls, independent aggregate counts were added and the restored target
rerun successfully. No broader cardinality census or shared decoder mutation
coverage is claimed. Counts are 91 ported / 35 divergent / 408 pending of 534;
this row is divergent for explicit ownership, type and response boundaries.

Final scoped host Clippy (`--lib --test legacy_history`) and separate wasm
Clippy (`--lib --target wasm32-unknown-unknown`) both exit 0. Changed-file fmt
and diff checks pass; real Yarn extractor check exits 0 with zero updates,
additions or orphans. No fresh full parity, CI or merged-state readiness claim.
The earlier transaction-slice parity result remains attached to that earlier
candidate, not relabeled as validation of this one. PR 7274 remains pinned.

## Integration response ABI merge

Merged integration `a5954aa869e368638986261e9ffa8829797ac60b` onto local
`cbf06de3d8e761e979e318b56fef218250ad336f`. Mechanical sub-agent review found
366 of 367 incoming paths exact; the factory fixture correctly retains the
incoming Encoded constructor and the local two-capture cardinality guard.
Named CI steps retain both parents (91/92 to 92). Real TOML review reports
370 exemption blocks / 370 distinct targets, no missing targets or blank
reasons; all eight WOO dependencies and layer containers remain intact.

The new legacy caller uses RestResponseBody::into_data, retaining native values
without serialization. Only strings enter HOW parsing. Three phase witnesses
retain all mixed-array members for Encoded, Untransformed and Transformed data.
The remaining FINDING is actual Axios transform equivalence, not missing shared
provenance. The historical byte-only finding above describes the earlier commit.

Merged-tree scoped legacy_history target: 3 passed, 0 failed, 0 ignored.
Real Yarn extractor check: 0 updates, 0 added, 0 orphaned. Changed-file fmt and
diff checks pass. No fresh host/wasm Clippy, full parity, CI or full merge-up
readiness claim; earlier sheets remain attached to their original commits.
Counts remain 91 ported / 35 divergent / 408 pending of 534.

## Outer transaction history: source-only capture

`history-fetch-oracle.cjs` executes the actual outer and legacy methods, candidate
selection, row selector, parameter builder and endpoint declaration. Its arm64
Node 24 corpus pins 72 cases / 328 request observations, fixed per-case lengths
and independent ordered route/phase/ordinal/parameter expectations. No-symbol
calls return directly from V3, including empty arrays and all five falsy thrown
values. Symbol-bearing calls exercise legacy success/error precedence, retained
V3 error identity, falsy overwrite and options mutation between phases.

Three source-only controls bypass the direct path, omit legacy and throw the
primary error too early; each executes and fails a named attempt assertion.
Returned-array and thrown-object identity are observed before JSON capture.
The previous legacy oracle is unchanged. This fake-XHR corpus measures method
control flow and query arguments, not actual Axios wire transformation. No Rust
implementation or compiling-control claim; no row classification changes.

## Outer transaction history native orchestration

`tests/history_fetch.rs` passes 2/2: 40 actual-source/native cases, 160 requests,
42 returned rows and 12 retained errors, with 32 explicitly excluded falsy-error
and mutable-options cases. The source corpus remains unchanged at 72/328.
Twelve independent primary numeric observations preserve signed zero and signed
infinities and refuse NaN before transmission; present page is omitted from V3
and retained by legacy, while missing bounds stay absent.

Six controls compile and exit 101: direct-route, omitted legacy and early primary
error fail `HISTORY native request sequence`; empty, subset and extra output fail
`HISTORY native row cardinality`. Restored target passes 2/2. Iteration one stopped
on an unused-import error in the legacy omission mutant, supplying no behavioral
evidence for that mutant. Iteration two creates an unpolled legacy future and
all six controls reach named assertions. No suppression was added.

The row is divergent for typed/owned boundaries and the existing unmeasured
Axios response-transform equivalence. No full parity, CI or merged-state
readiness claim; scoped lint results, when available, are reported separately.

Scoped host Clippy (`--lib --test history_fetch`) and separate wasm Clippy
(`--target wasm32-unknown-unknown --lib`) exit 0 under `-D warnings`.
Host iteration one found needless indexed access; `get()` retains the fixed
72-case loop bound without a lint exemption. Real Yarn extractor: 0/0/0;
layer check exits 0. Real tomllib census: 91/36/407 of 534 by source tree;
exemptions 370 blocks / 370 distinct. This does not claim a workspace or CI pass.

## Historical fills and trades caller capture

`historical-fills-oracle.cjs` executes both actual history callers, the actual
transaction parser, normalizers and omitUndefined dependency. The arm64 Node 24
source corpus has 48 cases, fixed 47 fetch calls, 96 parser visits, nine log
calls and 61 returned records. Each output field is checked before serialization;
only NaN bits are canonicalized. Input rows distinguish prices, quantities, fees,
PnL, maker and side fields. Signed zero remains bit-exact.

Direct fills preserve input order for same-second timestamps; fallback first
sorts trades by milliseconds, retaining that order during stable seconds sorting.
The initial fallback expectation was corrected from the executed source before
classification. Missing order IDs are omitted directly but synthesized internally
and retained through fallback. Pre-try normalization, whole-map-before-filter,
response-message precedence and an inner logger throw reaching the outer catch
are observed through calls and exact outcomes.

Six actual-source controls independently fail named assertions: limit clamping,
error-message precedence, omitted fallback, reversed seconds sort, early symbol
filtering and commission/quantity substitution. Every request field and thrown
error name/message is asserted independently.
This is a source-only capture with fake fetch and logging boundaries, not a Rust,
HTTP/Axios, Store notification or runtime-registration claim. No rows move.

## Historical callers: native composition

`tests/historical_fills.rs` passes 2/2. The original 48-case source artifact
remains byte-identical; 44 cases compare native fields before serialization,
44 logical fetches, 179 actual REST attempts, 61 output records and nine reports.
Four cases are explicitly excluded: two dynamic-symbol cases outside shared
options and two NaN-limit cases hitting the inherited JSON-parameter refusal.
The fixture uses the real DefaultStore, BaseExchange, REST helper and fallible
error listener, not a venue-local lifecycle or logger implementation.

The separate 18-case boundary supplement was added during implementation review,
not before implementation. It measures source asset aliasing and refusal timing,
plus native NaN and signed-infinite bounds. Rust returns 38 records and two whole
operation refusals against 44 source records, with 18 requests and two reports.
An incompatible asset discarded by slicing succeeds; one retained on output
refuses without source-error logging; a later parser error wins first.

Nine independent production mutations compile and fail named assertions:
public-data bypass, millisecond-sort replacement, price/quantity swap, early
filter, early output refusal, empty/subset/extra output and swallowed logger
failure. The restored target passes 2/2. Native iteration one failed fixture
construction (missing key), supplying no caller evidence; iteration two passed
44 cases, and the supplemented iteration passed both targets. Production sorts
are non-panicking and preserve equal-timestamp order. No full parity, CI or
merged-state readiness claim. Both rows remain divergent for named boundaries.

Scoped host Clippy (`--lib --test historical_fills`) and separate wasm Clippy
(`--target wasm32-unknown-unknown --lib`) exit 0 with warnings denied. The real
Yarn extractor check reports zero updated, added or orphaned symbols; layers,
changed-file rustfmt and diff checks exit 0. A real tomllib source-tree census
includes the journal: 91 ported / 38 divergent / 405 pending of 534. Exemptions
parse as 370 blocks and 370 distinct targets. Candidate sub-agent review returns
MERGE for this slice, not for the merged workspace or CI.

## Order pagination source phases

`order-pages-oracle.cjs` extracts the actual recursive source method and runs
22 fixed cases: 31 requests, 30 output rows and five reports. Capture headers
record architecture and Node version, with only NaN canonicalized. Request and
output content, independent counts, error identity and returned-array identity
are asserted. Disposal is checked after fetching; later reported failure retains
prior rows, whereas request rejection escapes. Current rows are copied before
awaiting recursion. Empty rows bypass metadata; malformed iterable phases and
fractional/NaN pages are separate cases.

Four source mutations fail named request-cardinality, output-order and message
assertions. Iteration one exposed cross-realm Array prototype comparison in the
observer; Array.from now produces host observations. Iteration two caught an
incorrect total expectation (28); the independent per-case outputs sum to 30.
Iteration three and all four controls pass their expected checks. This is a
source-only witness using fake xhr/disposal/reporter boundaries, not a Rust,
Axios, notification or row-completion claim.

## Native order-page candidate (verification in progress)

The iteration uses the existing RestAdapter, BaseExchange disposal and fallible
reporter. Boxed errors retain RestFailure or ExchangeError without projecting
metadata away. Shared JS coercion computes the page-size product before total
coercion. A code review caught the initial reversed coercion order and the
missing GET assertion; both were corrected before the native test completed.

A separate four-case source supplement observes terminal and recursive element
referents through later mutation, plus metadata coercion before iterable failure.
It exits 0; the original 22-case artifact is unchanged. This supplement was
written during native implementation and has no retrospective independence claim.
The native fixture admits 15 original cases and explicitly excludes seven:
NaN page parameters, live array mutation and five malformed-array/metadata cases.
No row moves until native assertions and compiling controls have executed.

The initial scoped lib check exited 0. Two accidentally overlapping queued test
commands were terminated using their captured Cargo PIDs after cwd verification;
both exited 143 and supply no test result. The single replacement scoped test
and eight independent mutation controls remain unclaimed until their results.

Native pagination's first completed target ran 2/3: the metadata assertion
incorrectly compared JsCoercionError's Display (including TypeError) to message
alone. The assertion now downcasts the retained error and independently checks
name and message. The replacement target passes 3/3, including the paired source
live-referent/native-owned-snapshot witness. The eight mutation runs are still
in progress; this result does not claim those controls have passed.

## Order wrapper source preparation

`order-wrappers-oracle.cjs` runs 24 actual-source wrapper cases against explicitly
fake recursive fetch, mapper and reporter boundaries. It checks 22 successful
output comparisons plus two reporter rejections, fixed per-case visit/output
counts, sparse input and mapper-driven append/removal. Limit output skips only
null; algo output accepts indexed array-like children. Mapper errors discard
accumulated rows and abort later mapping. Every returned mapper object, including
algo tails, is checked by reference before serialization.

Seven source controls fail named assertions: cloned mapper input, strict-null
substitution, skipped input row, separate limit and algo reversals, cloned algo
outputs, and error message precedence. Input reference observations also tag
sparse undefined independently. Review caught first-only identity and limit-only reversal
coverage; both were strengthened before classification. Capture headers name
architecture and Node. This preparation claims no native wrapper behavior,
actual-mapper equivalence, Axios behavior or row movement.

## Completed order-page controls

Measured code SHA: `2db1bc19b9d723eb2a6ccbb30c64c57249803fb5`.
Actual parent: `aa16188e8bebdbc5d423eea34ac9f585ef006d65`.
Inherited branch-only ancestors relative to locally tracked integration are
`799470ccda`, `26d0df7455`, `da71c8f3de`, and `aa16188e8b`; their whole-change
review status was not verified. This session's sub-agent review covered the
pagination change only. No integration merge or PR was performed.

The native serial runner completed iteration one through all eight controls.
Each mutant compiled, reached the named native assertion and exited 101:

| Control | Named failure |
| --- | --- |
| method | PAGES native request method |
| early-disposal | PAGES native request cardinality disposed |
| empty | PAGES native output cardinality direct |
| subset | PAGES native output cardinality direct |
| extra | PAGES native output cardinality direct |
| reverse | PAGES native row identity direct |
| lost-prefix | PAGES native output cardinality dispose-later |
| early-refusal | PAGES metadata coercion wins first over typed rows |

No survivors in this eight-control floor. After restoration, the scoped
`cargo test --manifest-path sdk-rs/Cargo.toml --offline --locked --jobs 1
-p safe-cex-venue-woo --test order_pages` exited 0: 3 passed, 0 failed/ignored.
It admits 15 of 22 original cases: 24 native rows, 22 requests and five reports.
The seven exclusions remain explicit; they are not silently counted as parity.
`/tmp/woo-pages-controls-evidence.json` records the measured SHA, command, source
hash, all eight exits and restored exit. Per-control logs are under
`/tmp/woo-pages-control-<name>.log`; these raw outputs are not repository artifacts.

The ordering supplement has four original boundary cases and two new phase
traces. Metadata property reads, size coercion, total coercion and row-iterator
access are checked in exact order. Distinct thrown objects establish which
failure comes first before serialization. Its `--controls` run exits 0 after
catching the named early-iterable assertion failure, then validating both
unmutated traces. Native competing faults preserve concrete JsCoercionError
name/message; the paired valid-metadata case reaches the later typed row refusal.
This is native error priority, not native executable getter instrumentation.

At the measured code SHA, source `order-pages-oracle.cjs --controls` and
`order-pages-boundaries.cjs --controls` both exit 0 (four original source controls
and the new ordering control). The scoped
`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets
--jobs 2` exits 0. The real Yarn extractor check exits 0 and its output reports
0 updated, 0 added and 0 orphaned symbols. No workspace compilation, host Clippy,
wasm Clippy/build, full parity suite, CI or installed Axios transform gate ran.

Sub-agent review found no blocking pagination issue. Its snapshot limitation is
retained: the native test changes the adapter's retained `data.clone()`, so it
witnesses owned Value snapshots and the source alias contrast, not live native
aliasing or copy timing during a suspended next request.

The follow-up manifest annotation moves one pending pagination row to divergent;
it does not relabel this tested SHA as covering the later manifest tree.
Real tomllib census against integration `2bc83d78d4` measured WOO's 22 manifests
(including journal) at 91 ported / 38 divergent / 405 pending of 534 before
that annotation. This is a WOO source-tree census, not a combined HOW census.

## Strict algo-order predicate: pending-row port

Measured code SHA: `7089a79c85c236e6e3e781ce851d2e50d1641c37`.
Actual parent: `b9f3699cba`; the inherited ancestry warning above still applies.
`orders::is_algo_order` now exists and uses the source's three strict string
comparisons, without coercing non-strings. Conditional remains false.

The actual-source oracle enumerates the independent six-value OrderType
inventory plus ten fixed unknown/missing inputs, stopping at 16 cases. Its
Node run at the measured code SHA exits 0 on arm64 Node v26.7.0. The Rust test
checks each native bool against a fixed independent expectation before checking
the decoded source result; it performs no encode/roundtrip comparison.

`order-policy-control.py` was invoked only after committing the candidate.
Iteration one removes stop_market from the actual predicate, compiles and exits
101 at `POLICY native classification at 3` (false versus true). No survivor in
this one-control floor. Restoration then exits 0: 1 passed, 0 failed/ignored.
The exact Cargo command uses `--test order_policy --jobs 2 --offline --locked`
under `-p safe-cex-venue-woo` and `--manifest-path sdk-rs/Cargo.toml`.
`/tmp/woo-policy-control-evidence.json` binds the command and both exits to the
measured code SHA; `/tmp/woo-policy-control.log` and
`/tmp/woo-policy-restored.log` retain the actual assertion and restored output.
The earlier development run before committing is not a SHA-bound gate claim.

Scoped changed-file rustfmt checks exit 0. Sub-agent review finds no blocking
predicate issue and verifies the fixed-count/source/native assertion order;
its review is not substituted for the executed native control.

The subsequent predicate annotation moves one pending row to ported, alongside
the pagination row's pending-to-divergent transition. Final manifest-tree check
and extractor results will be recorded against their own candidate SHA.

## Final manifest candidate and row movement

Measured manifest/code SHA: `539cf2bebe5dee8194a7b41bdf00b7a8b0ba9b5e`.
Actual parent: `7089a79c85c236e6e3e781ce851d2e50d1641c37`.
The final publication commit records this evidence only; it is not a fresh
test result for a different code tree.

Python 3.14 tomllib, over the 21 WOO exchange manifests plus the WOO journal
manifest, compares this committed candidate against integration `2bc83d78d4`:

| Census | Ported | Divergent | Pending | Total |
| --- | --- | --- | --- | --- |
| Integration baseline | 91 | 38 | 405 | 534 |
| Candidate | 92 | 39 | 403 | 534 |

**Moved 1 row pending-to-ported and 1 row pending-to-divergent.**
The exact transitions are WOOXExchange#isAlgoOrder and
WOOXExchange.fetchOrdersRecursively respectively. The source-file enumeration
stops at these 22 WOO manifests; it is not a combined WOO/HOW or global census.
`/tmp/woo-final-census.json` records both SHAs, parser, scope and keyed row moves.

At this candidate, scoped WOO all-targets Cargo check exits **0** with no errors:
`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets
--jobs 2`. The final real Yarn `parity:extract:check` exits **0** and its output
reports 0 updated, 0 added and 0 orphaned symbols. The writer was run after
each manifest edit, before committing the annotation; it reported 0/0/0.
`/tmp/woo-pages-final-check.json` and `/tmp/woo-pages-final-extractor.json`
bind the exact commands/exits/log paths to this candidate SHA.

Native mutation/restoration gates remain attached to their measured code SHAs
`2db1bc19b9` (pagination, 8 named exit-101 controls, restored 3/3 exit 0) and
`7089a79c85` (predicate, 1 named exit-101 control, restored 1/1 exit 0).
No new host/wasm Clippy, wasm build, workspace check, full parity suite, CI,
HOW runtime gate or installed Axios transform gate was run. Sub-agent review
covered the pagination slice and new predicate; inherited whole-branch review
status remains unverified as warned above. The unused inherited
order_mapping_error.rs draft and both local Aimux brief files remain uncommitted.

## Live algo mapper and fetch repair (wave 3)

Production `mapAlgoOrder` now executes the runtime-value mapper in
`binding/hosts/order_mapping.rs`. A partial STOP retains an own undefined type,
copied quantity/reduceOnly values, native arithmetic and the original parent
`info` reference. The actual HTTP `fetchAlgoOrders` member retains the STOP and
both bracket children (three rows); it previously returned zero after the
closed typed mapper rejected the STOP. Both unused typed order mappers and
their facade-only tests/controls are deleted. Only the genuine `is_algo_order`
predicate remains in `orders.rs`.

The two new real-class regressions first compiled and failed on the old dispatch
(`/tmp/woo-live-algo-before.log`). The maintained `algo-order-control.py`
compiles a closed-type regression inside the actual mapper, requires named
mapper and HTTP failures, restores exact source bytes, then passes all five
live mapper tests. `terminal-report-control.py` independently restores a closed
output refusal inside the actual limit mapper: all three terminal IOC/FOK/MARKET
socket assertions fail, then the restored stream suite passes. Neither control
replaces the production mapper or host with a stub.

The live unit suite executes all 103 actual-source mapper records, 36 native
numeric-bit witnesses and 12 filter/error/clock-order cases. Native observations
include own undefined fields, original prototype referents, UTF-16 values and
parent reference identity. The new TypeScript source witness and maintained
order-output oracle both pass. From the SDK directory with Rust 1.99.0,
`RUSTC_BOOTSTRAP=1`, one Cargo job and `SAFE_CEX_TYPESCRIPT_PATH` set to the
installed TypeScript module: library 16, `binding_orders` 36 and
`binding_streams` 15 tests pass (67 total). Strict native and Wasm all-targets,
all-features Clippy and workspace formatting checks pass. Global manifest
invention/status checks pass: 7,459 rows and 17,212 public Rust items. These
results concern live mapper/fetch behavior, not accepted-set contract coverage.

The shared Store/public typed-return boundary is still separate: measured,
uninstalled socket regressions reject undefined STOP type, string amount and
missing reduceOnly. No numeric coercion or row skipping is introduced to hide
that boundary. The shared model owner has the enum/price cutover; amount and
reduceOnly remain subject to the documented shared-type decision. The synthetic
string-quantity fixture proves TS behavior, not that a live legacy endpoint has
sent that payload.
