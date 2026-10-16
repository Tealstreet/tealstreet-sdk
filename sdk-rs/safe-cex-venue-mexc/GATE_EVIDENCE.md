# MEXC parity evidence

## Types and protocol tables

Measured starting census: 185 pending rows in six manifests. This phase maps
23 rows in `mexc.types.ts`: 21 constants, one classifier and one envelope type.
No declared divergence or accepted omission in this phase; 162 rows remain pending.

`mexc_tables_live_typescript_every_value` compares all 21 maintained protocol
tables, 10 channel classifications and 44 envelope accepted-set cases. The latter
are checked by the real TypeScript compiler with strict null and exact optional
property settings, and decoded through the public qualified Rust Deserialize call.
The envelope's private companion rejects positional arrays; optional fields reject
present null except unknown `data`, which retains it.

Independent controls change RECV_WINDOW from 20000 to 20001 in Rust and separately
in the maintained TypeScript source in memory. Both produce exit 101 at
`mexc_tables_live_typescript_every_value`, naming `table RECV_WINDOW`. Rust is
restored in a finally block; the restored run passes. The TS mutation is a child
test so the normal suite requires this failure and cannot silently skip the oracle.

The isolated-source mirror has no node_modules in any ancestor. Its oracle
passes with SAFE_CEX_TYPESCRIPT_PATH; mutating the compiler import fails with
MODULE_NOT_FOUND and removing the compiler path fails explicitly. This control
runs from the normal Rust suite and never reads dependencies from root node_modules.

Measured checks so far: table tests pass; scoped strict clippy passes; CI wiring
and layering suite passes 104 tests. Removal and shell masking of the MEXC oracle
step are required failures in that suite. The root script is `venue-oracle:mexc`.

Cargo.lock changes only MEXC's dependency list. Before/after cargo tree package
identities are identical: no added/removed package, version or resolution change.

The final three-test table suite and scoped wasm32 build pass. Global parity
currently fails on two pre-existing unmapped shared numeric items (`math_log10`,
`math_pow`) in the assigned base; this has been routed and no MEXC diagnostic
was emitted. It is not reported as a passing global gate.

## Independent pure utilities and signing primitives

496 live utility cases cover 21 functions; 58 API primitive cases cover four
functions (REST/WS HMAC, public-route classification, raw request body handling).
Every float is captured by a test serializer before JSON: finite/infinite/negative
zero bit patterns are explicit and NaN is tagged. Errors compare name and message.

The balance result reuses the shared `Balance.currencies` BTreeMap. Its values
match, but iteration sorts currency keys instead of JS insertion order. This is
one CHOSEN divergence to reuse the existing Store/domain boundary, not a forced
language restriction. The plain-object __proto__ setter is preserved by omitting
that non-own currency key after evaluating its row.

Independent Rust/TS contract-size fallback controls and Rust/TS HMAC-preimage
controls all fail their named live tests at exit 101. Restored executions pass.
The TypeScript controls are child tests in the ordinary test suite. The isolated
tracked-source mirror also runs all 496 utility and 58 API cases, with compiler
and real create-hmac missing-import controls. Runtime packages resolve beside
the isolated compiler, not through the root. Native cargo discovers these tests,
so create-hmac is an appended selector sourced from the owning package; existing
selectors and copied root lockfile remain unchanged.

Measured census after these maps: 47 ported, 1 CHOSEN divergence, 137 pending.
Ten timestamp-dependent utility rows and three sorted-query/client rows are
explicitly pending on shared Date.parse and localeCompare. No local shim exists.
Exchange and websocket behavior is not claimed by this phase.

The tables phase received a read-only review in a separate sparse checkout:
MERGE, no blocking findings. Its reviewer independently executed the Node table
oracle and isolated mirror; cargo/negative Rust build evidence was parent-run.

Phase 2 scoped all-targets tests: eight pass, none ignored. Strict clippy and
wasm32-unknown-unknown check pass. Combined CI wiring and layer tests: 182 pass
(the first invocation lacked cargo on PATH; rerun with the toolchain PATH passed).

## Reviewed follow-up and integration merge

The phase-2 reviewer found eager null-row validation changed which TypeError
won when an earlier row threw during currency/free coercion. Validation now
occurs at the find/reduce traversal point. Three live error-precedence cases
raise the corpus to 499; the plausible eager-validation substitution is covered
by a named Rust negative control. Nine crate tests pass, none ignored; strict
clippy passes after the correction.

The currency-order difference is a routed shared-types FINDING, not an
unobservable choice: BalancesModule.readAccountRows appends visible futures rows
in Object.entries order, QuickOrder exposes currencyKeys, and ModuleAPI returns
an aggregated currency map in insertion order. The committed acceptance witness
pins TS [USDT,USDC] versus Rust [USDC,USDT] and must flip on the shared repair.
The original CHOSEN mechanism remains documented; it is not accepted as parity.

Merged integration bf9e30f7c1 at 0b02e52a16. CI named steps: local 56, incoming55,
merged56 (distinct names41/40/41); every name in both parents survives. Scoped
crate tests compile and pass; layer audit passes. Global parity now passes:
6906 rows, 1166 public Rust items. Coverage extraction check reports zero changed,
added or orphaned symbols. This supersedes the earlier unresolved-math gate
failure. Collation and Date.parse are still absent and their rows remain held.

Final follow-up sub-agent verdict for 83c4064360: MERGE. Reviewer independently
ran the isolated 21-table/499-utility/58-API oracle and missing-dependency controls;
parent ran Cargo. Eager-null validation and contract-size substitutions both
fail the named live utility assertion at exit 101, then restored source passes.

Read-only exchange readiness audit found bf9e30f7c1 still has only BaseExchange
fetch_and_set_orders/fetch_leverage_for_symbol; the required dyn Exchange hooks
land separately at d854ac9ebd, with worker dispatch at 23e34c493f. Their integration
SHA is routed before the exchange phase. No base bypass was introduced.

## Registered collation and query primitives

Merged integration 35e1b03e7f and core 1d23618857, at dc501b99d7 and 99c3791ccc.
Resolved CI/exemption conflicts by union. Named CI steps:56 local,62 integration,
55 core;63 merged, with every parent name retained. Scoped crate compilation,
layer audit and114 CI-wiring tests pass. Core dynamic override hooks now exist.

serializeMexcParams and getQueryString compare across162 live API cases (including
the58 prior primitives). Stable shared root collation applies at every object and
array depth; qs repeat arrays, percent encoding, BigInt/nonfinite number spelling,
prototype-named own keys and raw URL query fallback are compared. Independent
Rust lexical-sort and indexed-array substitutions each fail the named live
assertion at exit101; the in-memory TS reverse-sort control also fails by name.
The existing HMAC preimage controls continue passing their required failures.

The71-scenario live factory boundary oracle executes the maintained createAPI
with real Axios, axios-retry, qs, create-hmac, JSON parser and retry classification.
It injects only the shared transport factory and clock. It captures HMAC preimages
AND final wire data, routes, headers, retries and response-bearing errors; it is
measurement for the pending client, not evidence of a completed Rust factory.

FINDING: shared RestBody wire_bytes emits raw strings regardless of content type.
Axios JSON transform signs raw text unchanged then sends a quoted JSON string;
empty string is quoted; valid JSON strings are trimmed on the wire while signing
retains whitespace; BOM-prefixed JSON fails parsing and is quoted. The committed
mexc_factory_shared_json_transform_finding pins the mismatch, and a TS content-type
mutation must fail that named witness at101. The core owner has this routed.
createAPI stays pending instead of installing a venue-local pre-signing transform.

The tracked-source no-ancestor-module mirror covers21 tables,499 utilities,
162 API cases and71 factory scenarios. New axios,axios-retry,qs selectors use the
owning package manifest and copied frozen root lock; no existing selector changes.
Twelve scoped tests pass, no ignored tests. Strict clippy and scoped wasm check
pass. Coverage check: zero changed, added or orphaned symbols. Existing Math.max
sites now call the newly merged shared math_max primitive. Census:49 ported,
1shared-types FINDING (currency order),135 pending. Ten timestamp rows await
Date.parse; one client row awaits the shared transform; exchange/WS remain later.

Query review found duplicate JsJsonValue::Object entries were emitted twice.
The serializer now applies the shared value contract: first key position, last
assigned value, once per key. Six entry-list cases (24 calls) survive the JSON
oracle transport using an explicit entry tag; the corpus is now186. Restoring
the duplicate-key behavior fails serializeMexcParams by name at101. Twelve
scoped tests and strict clippy pass after the fix.

Representation audit also corrects three overbroad ported claims: query encoding,
query selection and request-data normalization use the owned JsJsonValue tree,
which cannot represent Date/functions/accessors/toJSON hooks/cycles. These are
CHOSEN input-domain restrictions, justified by reuse of shared transport data;
the shared UTF-8 restriction is separately cited as shared-utf8-string-boundary. That
citation is not used to excuse the other exclusions or ordering. Current census:
46 ported,4 divergent (three input-domain choices plus shared currency-order
FINDING),135 pending. No newly completed client is claimed.

## Shared Date.parse and HTTP transform integration

Merged integration a99f1fffe1 and core f018fa189d. Workflow named steps are
68 after the union, preserving all 63/63/59 parent names. Scoped crate check,
37-crate layer audit and 155 CI wiring tests pass. Cargo regenerated only this
crate's hmac/sha2 dependency qualifiers; no package/version/resolution moved.
The 71-scenario live factory witness now requires equality at the shared
content-type-aware wire boundary, while retaining the preimage assertions.
createAPI remains pending until a Rust factory exists.

Five timestamp functions now consume shared V8 Date.parse with an explicit host
local-wall-time callback. Utility corpus is 595 with IEEE bits retained. Nine
UTC/Singapore/New York local-time cases pin callback input and output including
DST gap/fold; date-only and offset timestamps bypass it. Rust zero-fast-path and
TimeClip substitutions each fail the named utility test at 101, then restored
code passes. An independent TS zero-fast-path substitution also fails by name.
Six utility tests, six API tests and four table/isolation tests pass. Sequential
review found a transient UTF16 surrogate from indexing an ordinary astral string;
that intermediate now converts to numeric fallback zero as in TypeScript.
Both surrogate halves and a following digit are live cases. Reviewer rechecked
and returned MERGE, with no concurrent cargo.

Lookup inventory: utils10 (numeric-key immune), API1 (closed BASE_URL),
exchange15, publicWS1, privateWS0. The live oracle pins 27 source sites and52
pre-serialization witnesses across four caller-option expressions and all twelve
prototype names plus a miss. The affected consumers remain pending; this is an
acceptance witness, not a claim that TableHit integration is implemented.
Current manifest: 51 ported,4 divergent,130 pending of185.

## Remaining pure mappers and shared retry acceptance witness

TableHit integration6eb262afc5 and retry ABI260f3bcba4 merged by union:71
named steps preserve68/66/59 parent sets. Scoped compile,37-crate layer audit,
169 CI wiring tests, global parity6967rows/1246public items, and zero-drift TS
extraction pass. All19 MEXC tests pass; clippy all targets with warnings denied
passes after removing two redundant string conversions and a lazy error closure.

The remaining five mappers have269 maintained live TS cases. Five cases explicitly
witness the commissionAsset output-domain FINDING (null,false,zero,object,array).
No local widening or silent stringification. Four Rust substitutions (regular
vs trigger states, full-position stop sizing, remaining clamp, margin division)
and a TS trigger-state substitution each fail the named mapper test at101.
Review found the second numeric coercion on regular-order and sized-stop volume:
string '-0' becomes numeric+0 there, while plan orders retain-0. Fixed only the
applicable sites, added all five mapper witnesses, and reviewer rechecked MERGE.

The 52 inherited-member witnesses now compare shared TableHit truthiness,
nullishness and pre-serialization function/prototype identity. This does not
claim the still-pending exchange/socket consumers are implemented.

HEAD retry body-state is a separate shared seam from the repaired JSON transform.
Three live Axios cases sign raw text, spaced valid JSON, and empty string on the
first attempt; the second signs the quoted/trimmed/quoted-empty transformed body.
Wire bytes are identical across attempts. mexc_head_retry_shared_body_state_finding
pins shared RetryPolicy's original-body replay against those preimages. Core has
the repair queued; createAPI stays pending. Sequential reviewer approved this
witness independently; no concurrent cargo.

Current census:55ported,5divergent,125pending of185. Pure utility rows have no
pending entries. Exchange class and sockets remain unported; factory has the
explicit shared retry blocker.
Scoped wasm32-unknown-unknown check also passes after the shared crypto merges.

## Stateless exchange helpers

Eleven helper implementations have414 maintained-source cases. The oracle takes
field initializers and constants from the real class, uses its real OrderType
values and evaluates their helper dependencies. Two Rust substitutions (plan
classification and trigger direction) and a TS plan-classification substitution
each fail the named helper assertion at101; restored code passes.

Review found string-key indexing on nested arrays in extractRows. Follow-up
measurement also found Array.prototype under __proto__: string0 returns[1,2],
__proto__ returns[], and noncanonical00/-0 fall through to[7]. Rust currently
returns[7] for both findings. Exact TypeScript and Rust outputs are pinned;
extractRows is divergent/FINDING, not ported. Current source callers use only
empty keys or orders/items. The owner routed canonical numeric property access
and explicitly held the array-prototype representation; no local prototype map.
The other ten helpers passed sequential review. Reviewer approved recording the
precise unresolved finding rather than claiming parity.

The six affected helper/table tests and isolated full oracle mirror pass;
global parity passes6967rows/1257public items. Census65ported/6divergent/114pending.

## Cancellation shields and response guards

Eight exchange shield helpers are explicit state operations over the venue's
private ID maps. The maintained source oracle executes 16 traces / 304 steps,
asserting return values, all three map states and clock-read counts after every
operation. It covers empty IDs, suffix stripping, prototype-spelled IDs (Map
keys, not object-table reads), time zero, backwards time, expiry equality,
expiry deletion, cancellation clearing placement state, and a clock advancing
between two records in one filter. Iteration order of these private maps is not
read by the source; map comparisons therefore compare keyed state.

Review found a frozen per-filter clock that would retain both orders across the
expiry boundary. The implementation now reads the injected callback at each
source clock site, including no read on a missing ID or a short-circuited regular
order. The regression retains the first canceled entry but expires the second.
Four independent compiling Rust substitutions (inclusive expiry, retained
placement on cancellation, removed empty-ID guard, frozen filter clock) fail
`mexc_shields_live_typescript_state_transitions` at exit 101, then restored source
passes. The ordinary suite also requires a TypeScript expiry mutation to fail.

Two rejection guards compare 132 live cases (103 errors) by name and message.
The cancel-row filter completes before formatting any failed order IDs/messages;
an earlier unstringifiable ID cannot win over a later null-row error. Strict
success-only and eager row formatting substitutions both compile and fail
`mexc_exchange_errors_live_typescript` at exit 101, then restored source passes.
The maintained TypeScript strict-success mutation is a required child test.
Separate sequential reviewer passes approved shields and rejection guards.

The isolated tracked-source mirror includes both new oracles and retains its
missing compiler/runtime import controls. All maintained oracle surfaces pass
without any node_modules ancestor. The native crate suite now has 25 passing
tests, none ignored. Census: 75 ported / 6 divergent / 104 pending of 185.
No factory, full exchange constructor or socket completion is claimed here.

Integration/core merge sequence: e002c746b2 (35b7ec85f9), 6217e72557
(86aae2c4e1), f62ceee6d5 (590f1a4d7e), dd4fbed449 (d8bcccd413).
Named workflow steps retain every parent: 71/70/59 to 71, then 71/70 to71,
then71/71 to72. Shared exemption entries are273, unique and nonblank. The
590f1a4d7e lock omitted candle-cache's Redis/Tokio graph; the proposed offline
repair was saved and reverted, then the canonical repair was merged from
integration. No venue-authored shared dependency resolution was committed.
CI wiring passes180 tests and the layer audit covers37 classified crates.
Strict all-target clippy and scoped wasm32-unknown-unknown check pass after the
canonical lock repair. Global parity passes (6984 rows / 1300 public Rust items).
The new rejection source file is explicitly registered in the exchange manifest;
the first parity run caught that missing declaration before it was corrected.

## Factory composition and shared repair acceptance

The committed retry-body finding flipped by name at exit101 after integration's
repair. Its replacement compares preimages AND wire bytes for three HEAD body
shapes across both attempts. `client::create_api` now composes the real shared
HTTP client, signer adapter, response parser and classified retry policy.
Factory comparison covers71 maintained source scenarios, three retry shapes and
four numeric-response cases, including positive and negative overflow. Remote
identity fixtures exercise the shared composition, not a real remote signer.

Sequential review found that the shared HOW parser erases response negative
zero. The original factory JSON output concealed it; the oracle now observes
all result numbers as IEEE bits before serialization. Negative-zero responses
are explicit FINDING acceptance witnesses; only that sign may differ. Shared
header findings separately pin Axios case-alias merging and implicit Accept.
All unaffected headers compare exactly. These findings keep createAPI divergent.

Four Rust controls substitute public-with-params signing, HEAD query signing,
success-only rejection and shortened retry count. Each compiles and fails the
named composition assertion at101; restored source passes. The ordinary suite
requires an in-memory TS content-type mutation to fail the named child test.
The isolated tracked-source mirror now covers78 factory cases; missing compiler
and runtime-import controls fail loudly without any node_modules ancestor.

extractRows now calls shared js_object_or_array_entry; its nested-array numeric
key witness is positive. Substituting object-only lookup compiles and fails the
named helper assertion at101, then restored source passes. Array.prototype is
the remaining precise FINDING and current production orders/items keys are
unaffected. Three helper controls pass this discipline.

Integration9222ed2454 is merged at64db7e8439. Workflow union72/73->74 retained
every named parent step. Real tomllib parsing reports280 exemption blocks and
280 distinct targets, zero blank reasons. Full scoped wasm Clippy with
--all-targets --jobs2 -Dwarnings passed including dependencies after the shared
candle-cache repair; this supersedes the earlier --no-deps-only measurement.

Final sequential review accepted the declared-divergent factory after inspecting
the numeric fix. Scoped native crate tests:28 passed, none ignored. Host strict
all-target Clippy passed. Layer audit:37 classified crates. CI wiring:188 passed,
zero skipped. Extractor:0 changed manifests,0 added,0 orphaned symbols.
Census:75 ported /7 divergent /103 pending,185 total. Constructor, full exchange
orchestration and sockets remain pending; the reconciliation oracle is a draft.
Global parity passes6984 rows/1354 public Rust items. Final host and full wasm
strict all-target Clippy both pass separately. Scoped rustfmt check passes.

## Order fetch and reconciliation

MexcExchange now owns the fetch and snapshot methods, with constructor/sockets
still pending. Maintained source oracles execute actual DefaultStore constructors
on both sides:112 reconciliation cases and220 fetch cases. Snapshot comparisons
include returned rows, stored order and duplicate priority, loaded flags, update
IDs, notification counts, all three shield maps and disposal after the fetch.
Fetch cases retain per-scope request order/params, mapper numeric bits, caught
HTTP failures, malformed rows and coercion errors. Source error-message
coercion can itself throw; reviewer found a swallowed failure and the fix now
preserves TypeError and prevents both logging and later requests.

Seven compiling controls fail named tests at101: swallowed message coercion,
base-hook bypass, stale disposal capture, fetched-first regular snapshot,
placement release on the wrong snapshot, ninety-day lookback and symbol dedupe.
Restored source passes. Mandatory child tests mutate disposal and TS dedupe.
Dynamic Exchange dispatch and a captured concrete method reference both prove
MEXC's placement shield runs. Unpolled and pending reconciliation futures can be
dropped without changing Store or shields, and the next snapshot still works.

Both reconciliation returns are CHOSEN owned-snapshot-vs-live-alias: mutating
returned lastUpdate changes the TypeScript Store row; Rust's returned owned row
leaves Store unchanged. Both views are captured after the same live operation.
The source's disposal boundary calls the real Store reset, matching BaseExchange.
Sequential review cleared the seven mappings after the coercion and manifest
fixes:five ported and two declared divergent. Census80ported/9divergent/96pending.
The isolated no-ancestor-module mirror passes both new oracles and existing
compiler/runtime missing-import controls. No full constructor claim is made.

## Consolidation merge-down

Integration91db928805 merged at71ff0eef8b; KuCoin/GRVT integrationb1ef25ef36
merged at879601d7a1; shared get-key integration6bcff79504 merged at7f551064f7.
Named workflow step multisets retained both parents:74/76->77,77/78->79,
79/78->79. Real tomllib parsing reports303 exemption blocks and303 distinct
targets, no blank reasons and no parent target omitted. Layer audit passes37
classified crates. Additive CI assertions and isolated npm selectors were
unioned; wiring passes203 assertions. MEXC retains its11 direct dependencies.

Core's account-options owner requires construction-time options and venue-aware
public-data-only dispatch; the existing220-case fetch oracle still passes both
public/private cases. Shared Axios header and negative-zero repairs each flipped
the committed FINDING by name at101. The old exception assertions were replaced
with positive whole-header and pre-serialization numeric-bit comparisons over
all78 factory scenarios. No venue-local normalization was introduced.

Scoped crate tests pass33, none ignored; host strict all-target Clippy and
wasm32 strict all-target Clippy pass separately, including dependencies.
Wasm Clippy was also rerun after the final get-key merge. Scoped rustfmt passes.
Extractor reports0 updates/0 added/0 orphaned; global parity passes6984 rows
and1633 public Rust items. Merged-state review requested separately; readiness
requires its returned verdict and a fresh integration-head check.

Merged-state adversarial review cleared CI multiset unions, all crate dependency
and lock entries, selectors, exemption reasons and layer containers. It blocked
readiness on seven mapper alias contracts. The correction declares six formerly
ported rows CHOSEN and adds aliasing beside Balance's separate ordering finding.
Seven maintained live-source cases observe both input-to-output and output-to-input
mutation, at the top level and nested level. Plan/stop spread only retains nested
aliases; the other four mappers retain the original info object, and Balance
retains both aggregate rows and currency-row info. Rust snapshots remain unchanged.
A mandatory child control replaces source Market info with a clone and fails
the named alias-direction assertion at101. The isolated source mirror includes
this new oracle. Revised census74ported/15divergent/96pending,185 total.

Final incremental merge review cleared d862e94f82 against both parents, including
the seven corrected alias rows. It required one additional Balance note:
__proto__ writes change the TypeScript currency-map prototype and expose numeric
inherited fields. The new maintained source witness records currencies.free=7,
prototype total=9, no own free key, and a shadowed constructor currency; Rust
retains the own constructor currency but has no inherited free entry. This is
a third separate routed FINDING, alongside currency ordering and aliasing.
The targeted three-test alias suite passes; no mapper behavior was changed.
The merged tree passed35 scoped tests before adding this one test,212 wiring
assertions, host and wasm strict Clippy separately, extractor zero drift,
layers37, and global parity6984/1852. Integration fresh-fetch remains8cbfb8b5f9.
Final sequential review of eb77842d44 independently ran the prototype oracle and
cleared the delta with no remaining actionable findings. Host and full wasm
strict Clippy passed again after the new witness. The isolated tracked-source
mirror passed all oracles, including seven alias cases and prototype reads,
with mandatory missing-compiler and missing-runtime failures retained.

## Existing numeric ingress audit and shared Balance repair

The existing MexcEnvelope map-only decoder has two distinct routed findings.
Already-parsed Value integer lexeme -0 loses its sign in code and unknown data,
while native Value(-0.0) and lexical -0.0 preserve it. Independently, its untagged
MexcCode refuses valid numeric spellings that Node accepts: raw -0/-0.0, decimal
1.5, exponent1e3, and both overflow signs; existing Value1e3/overflow also refuse.
The maintained declaration's optional number|string union is checked by the
live Node oracle, and nine JSON cases compare IEEE bits across raw, existing-
Value and native routes. Unknown-data raw parsing is checked independently of
the code-union refusal. Six raw refusals and three existing-Value refusals are
pinned acceptance witnesses, not approximations or Value-first workarounds.
MexcEnvelope is now divergent; census73ported/16divergent/96pending.

Integrationb8fc4dfbaa adds the shared JsObjectMap. After the necessary ABI-only
construction/iteration adaptations, mexc_balance_shared_currency_order_finding
compiled and failed its named assertion at101: Rust now produces USDT,USDC.
The renamed positive parity test requires equality with live source. Only the
ordering finding was removed; Balance's alias choice and prototype-write finding
remain separate. No venue-local map implementation was introduced.

This merge preserves83 named CI steps against82/82 parents, and a real tomllib
parse reports312 blocks/312 distinct exemptions with every parent target retained
and no blank reasons. Wiring220 passes; scoped tests37 pass, none ignored;
host strict all-target Clippy and wasm strict all-target Clippy pass separately,
including dependencies. Extractor drift0, layers37, scoped rustfmt pass.
Sequential review cleared the numeric ingress witnesses and merged-state
preservation at30b5cea4c0, independently checking the Node oracle,83steps and
312distinct exemptions. Readiness remains held on the routed numeric ingress
findings as directed. Global parity passes7334rows/2259public Rust items.

## Pure symbol configuration

deriveSymbolConfig and getLeverageForOrder now have native helpers with explicit
positions, cached settings and account mode. A maintained-source oracle executes
both class field bodies and the actual DefaultStore constructor over180 setting
scenarios and720 order selections. Numeric observations retain IEEE bits:
zero/negative/nonfinite contracts, leverage fallback, one-way rails, source
ordering, cached short-circuiting and malformed info coercion are covered.
No wire decoder or new shared numeric conversion is introduced.

Five independent compiling controls fail the named integration assertion at101:
wrong position mode, positive-only contracts, native numeric Display retaining
negative zero, inverted closing side, and ignoring cached configuration.
A mandatory child mutates the source one-way branch and fails by name.
The isolated mirror initially failed loudly because the new oracle implicitly
loaded virtualClock.ts; an explicit host-clock boundary matching the other
actual-Store oracles fixes that, and the whole scoped suite passes end to end.

Scoped tests39 pass, none ignored. Host and full wasm strict Clippy pass
separately, with dependencies included; scoped rustfmt passes. Extractor0 drift,
layer audit37. Two pending rows become ported:75ported/16divergent/94pending.
The concrete async leverage hook remains pending and will consume these helpers.

## Merge-up readiness on the six-consolidation integration head

Directly merged `feat/safe-cex-rust` at `6f419e0992`; resulting merge commit
`e2863009af7c6ae2560b5b0d8425a10a8e693d26`. The MEXC inventory parses to 75
ported, 16 divergent and 94 pending out of 185. No claims were added for the
still-pending leverage-fetch override or the shared numeric ingress finding.

On the merged tree, all 39 scoped crate tests pass. Strict all-target host
Clippy and strict all-target `wasm32-unknown-unknown` Clippy pass separately,
including dependencies. Parity passes all 7,334 rows and resolves 2,504 public
Rust items. The extractor reports 7,334 symbols and zero added/orphaned drift;
the layer audit passes for 38 classified crates. Rust CI wiring passes 230/230.
A real `tomllib` parse reports 312 exemption blocks and 312 distinct targets,
with no blank reasons.

The merged-state adversarial review found no preservation blockers. It verified
the exact max-union of both parents' 83/85 CI steps (86 merged), both parents'
89/90 root scripts (91 merged), 511 manifests/7,334 rows, 312 unique
exemptions, dependency resolution in all 520 lock packages, and the unchanged
layer/exemption containers. Review completed before the post-merge Cargo builds.

## `findOpenPosition` pure helper

Added `find_open_position` as an order-preserving typed-position helper. Its 26
maintained live-source cases cover wrong symbol and side, empty and nonempty
no-match, and contract values -1, -0, +0, NaN, +Infinity and positive finite.
The 440-case helper oracle passes in the tracked-source mirror without ancestor
`node_modules`. Rust mutations changing `contracts > 0` to `>= 0` and adding a
last-row fallback, plus a TypeScript source mutation, each fail the named
`findOpenPosition` assertion at exit 101. Scoped helper tests pass 3/3.

## Continued pure-helper work after MEXC consolidation

MEXC is now based on integration `56e818d52b` (Woo above the MEXC merge).
`findOpenPosition` is ported as `find_open_position`; the venue inventory is now
76 ported, 16 divergent and 93 pending. All 40 scoped crate tests pass. The
updated 440-case helper oracle passes in a tracked-source mirror without
ancestor `node_modules`, and
all 26 new cases compare the first matching position ID against live TypeScript.
Parity passes 7,334 rows/2,506 public Rust items. Strict host and wasm32
all-target Clippy pass separately.

The envelope witness explicitly instantiates `MexcEnvelope<Value>`: direct raw
data-only JSON preserves the integer `-0` bit through `Value::deserialize`,
while the numeric code union refuses raw `-0` and `-0.0`. Rebuffering an
already-parsed Value through the generic wrapper changes both code and data to
+0. The paired `-0` and `-0.0` cases isolate the loss to the existing-Value
rebuffer path, not the generic Value container; they strongly
implicate integer-versus-float Number representation without proving the
internal serde_json cause. MEXC's other numeric mapping paths use source
`Number.parseFloat(String(value))`; the only typed protocol-table decoding
targets closed enums.

## Leverage payload formatter

Added `format_change_leverage_payload` with 112 maintained live-source cases
covering both margin types, position types 1/2 plus the source's other-value
short-side branch, both `includeOpenTypeWithPositionId` states, competing and
nonmatching positions, id fallbacks, negative zero and non-finite leverage.
The helper delegates selection to `find_open_position`, and the test encoder
asserts native f64 bits before JSON serialization. Rust and TypeScript
mutations are required to fail the named helper assertion at 101. The combined
exchange-helper corpus is 552 cases.

## Numeric-safe MEXC envelope ingress

After shared numeric ingress landed at `c659b17b74`, the envelope opts in via
`decode_mexc_envelope` and `decode_mexc_envelope_value`, with `JsNumberInput`
retained as generic `data` until the caller decodes its typed output. The
9-spelling live Node corpus now proves code and data bit equality on raw and
existing-Value paths, including `-0`, `-0.0`, decimal, exponent and both
overflow signs. The old direct-Serde mismatch stays as baseline evidence. A
TypeScript source mutation removing the numeric code member is required to fail
the named acceptance assertion at exit 101.

## Leverage override hook

`MexcExchange.fetchLeverageForSymbol` now dispatches through the concrete
`Exchange` override. A source-extracted live TypeScript oracle covers successful
long/short rows, the positive max-cap update, public-only and missing-market
fallbacks, and request-failure fallback; the Rust test compares returned values
and Store updates and verifies the endpoint params. Both an implementation
leverage and metadata delegation mutations, in both Rust and TypeScript, fail
named assertions at exit 101. Scoped Cargo testing and strict clippy remain part
of the merge-up run.

The same oracle constructs the actual TypeScript `DefaultStore` and verifies
the routed subscriber-notification finding (`097041a8c7`): a throwing first
listener runs twice, the second listener is skipped, the market cap and then
fallback fetched leverage are committed, and the second throw rejects the
method. At that capture the old Rust `Store::update` returned unit; the later
notification cutover made these writes fallible. The reconciliation oracle
now separately installs a throwing subscriber for each order writer and checks
the rejection, committed orders/loaded state, and skipped second listener.
The child control removes both source snapshot writes and must fail the named
subscriber-rejection assertion at exit 101.
The existing alias choices on the order methods remain separately recorded.

## Store notification and subscriber identity

Merged the notification ABI and audited all ten direct notifying Store
mutations plus the venue's `BaseExchange::log` calls. The three collected
compiler sites use result propagation: the order-fetch error arms propagate
`BaseExchange::log`, and the reconciliation subscriber returns `Ok(())`. The
market-cap notification remains caught and logged before configured leverage
fallback, matching the TypeScript catch path. Two targeted witnesses pass:
`mexc_websocket_store_notifications_propagate_after_commit` and
`mexc_reconciliation_live_store_transitions`.

The shared `ExchangeErrorMetadata.store_subscriber_ids` field now retains IDs
through both conversion paths, appending inner-to-outer. MEXC has no local box
to simplify: the existing `StoreSubscriberError` cause preserves the original
error payload, while the structural ID vector supplies notification identity.
The ticker and position witness now asserts ID `[0]` from each fresh Store. The
rejected-order witness also asserts ID `[0]`, committed canceled state and shield,
and that error emission stops after notification failure. `noteRejectedRegularOrder`
moves from divergent to ported, as does `handleTicker`; order snapshot rows retain
only their independent alias divergence. Other notification findings remain
where multi-write or route-level ordering still lacks a venue-specific witness.

## Price/amount precision swap audit

The existing 595-case utility target already detects swapping the production
`amount_step` and `price_step` assignments: the compiling swap fails at
`normalizeMarket` case 234. Direct native-field assertions now pin both precision
fields across the 11 successful live-source market cases; the swapped amount field
fails `CHASER-PRECISION native amount step case 234` with exit 101, and the restored
target passes. No corpus bytes or fixture values changed, and this corpus feeds only
this crate's tests. Rust order-write formatters remain pending, so no compiled
two-axis formatter site exists to swap yet; these assertions are a precondition for
those formatters, not a claim they are swap-proof. This establishes that the corpus
catches the swap now; I have not established that its discriminating cases predate
the earlier general identity sweeps, so I do not claim the corpus was never blind.

### Per-field precision audit

The TypeScript source has nine `adjust` calls in five formatter paths: regular
payload, plan payload, full-position stop payload and optimistic regular-order
reconstruction each adjust both volume and price; optimistic stop-order
reconstruction adjusts only price. Rust has no corresponding order formatter
implementation yet, so those four two-axis paths have no Rust production site
to swap and are not covered by the market-metadata result above. The current
Rust consumer exposes `Market.precision.amount` and `Market.precision.price`.
The 595-case live utility corpus contains 11 successful market outputs and is
consumed only by this crate. Independent compiling mutations now route amount
from the price step and price from the amount-derived step; each exits 101 at
its own direct native assertion (`CHASER-PRECISION native amount step` and
`CHASER-PRECISION native price step`). Both restore to the passing corpus above.
The earlier paired assignment swap also compiled and failed case 234. No corpus
bytes or fixture values changed in this per-field audit. The corpus catches the
two metadata identity errors now; its pre-sweep baseline is not claimed to have
been discriminating.
