# GRVT verification receipts

## Native metadata-only market refresh

The public `&dyn Exchange::refresh_markets` call now delegates to the existing
GRVT receiver override (`grvt.exchange.ts:803–823`). The shared default updates
only on ID changes, whereas GRVT always replaces the stored markets before any
membership event or log. Retaining unchanged IDs must not retain obsolete tick,
amount limit or ih/bd/qd signing metadata.

The constructed metadata-only regression compiles and fails without the native
override at `refresh must replace metadata` (tick 0.1 versus 0.25). Restoring the
override passes both new trait regressions, all 21 existing constructed binding
tests and all four actual-source/native market tests. The second regression
checks native added/removed Market results and committed Store membership.
Response changes are queued before the initial fetch so the scripted transport
actually returns the updated instrument on refresh; metadata exponents are
asserted as native Number bits rather than JSON integer spelling.

The four inherited-refusal policy records retain identical method bodies,
compiler APIs, TypeScript sources, fixture bytes, adapters and expectations.
Their whole-file source hash and +13-line spans were independently reviewed
against a frozen candidate by the shared accepted-set owner.

Fresh complete stored-owner gates pass on native and actual Wasm: each reports
117 production runtime results, 17 explicit classifications and zero failed
owners, including all 19 compiler aliases. The 268 receipts pass the scoped
ratchet. Strict host all-target/all-feature and Wasm all-feature clippy, package
formatting and three pure inherited-policy tests also pass. This verifies the
existing 134 stored owners; the remaining unreviewed owners stay open.

The complete `venue-oracle:grvt` command also passes: 245 tests across the unit
and 42 integration binaries, plus two compile-fail doctests, with no ignored
tests. Set both `SAFE_CEX_TYPESCRIPT_PATH` and `SAFE_CEX_CRYPTO_ORACLE_DEPS` to
their absolute installed package/dependency directories; the builder-address
oracle refuses an omitted crypto installation before evaluating source.

Reproduce the 27 scoped tests with `SAFE_CEX_TYPESCRIPT_PATH` set to the actual
TypeScript package, using `cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml
--locked -p safe-cex-venue-grvt --jobs 2 --test exchange_binding --test markets`.

## Protocol table slice

Base `9222ed2454`; five newly mapped rows, 158 remaining pending (one pre-existing
ported row). The live oracle executes the complete actual `grvt.types.ts` module
through an explicitly supplied isolated TypeScript compiler. It compares all
44 entries, including order, from the five URL/endpoint/telemetry exports.
No protocol schema or runtime exchange behavior is claimed by this slice.

- Node 24.20.0 and 26.7.0: `protocol_tables_match_live_typescript` passes.
- Independent Rust endpoint mutation (`all_instruments` to `wrong-endpoint`):
  compilation succeeds; Cargo exits 101 at `GRVT protocol table equivalence`.
- Independent live TypeScript endpoint mutation: run with
  `GRVT_PROTOCOL_MUTATION=endpoint`; transpilation succeeds; Cargo exits 101 at
  the same named comparison. This mutates executed source, not an expected fixture.
- Both mutations restored; baseline test passes again.
- Crate-scoped fmt, all-target tests including ignored, host strict Clippy pass.
- Separate wasm32-unknown-unknown all-target strict Clippy (without `--no-deps`)
  and wasm build pass. The Node integration test is explicitly host-only.
- CI/layer audit: 190 tests pass, zero skipped/todo. New controls reject deletion
  and failure masking of the mandatory GRVT step. The root script pins locked,
  all-target Cargo tests, two jobs and inclusion of ignored tests.
- Global parity-check passes: 6,984 rows / 1,275 public Rust items; parity-report
  runs successfully. No exemptions added; these tables map to real TS exports.
- Extractor check: zero changed manifests, zero added and zero orphaned symbols.
  The missing local smol-toml dependency was installed from the frozen root lock
  under this worktree's target directory; no main-checkout files were changed.
- A disposable tracked-source mirror outside all worktrees, with no ancestor
  node_modules, produced all 44 oracle entries (SHA-256
  `5f350c9dd9a03289c21a546600bbed267911cc5979d3047a3c770b0c98a16705`).
  A missing explicit compiler path failed with MODULE_NOT_FOUND. Mirror deleted.

Commands use `--manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-grvt
--jobs 2`; no workspace build was run. The inventory in LOOKUP_CENSUS.md is
investigative evidence, not a claim that inherited-member behavior is ported.

Sub-agent review: MERGE in an isolated checkout. Its nonblocking suggestion to
anchor the GRVT step audit to the native job was applied, with a mutation control
rejecting relocation outside that job.

## Protocol string-enum slice

Six enums map five TS literal unions plus GrvtCandlestickInterval. The actual
TypeScript checker supplies all 35 accepted strings; 595 derived probes reject
non-string payloads, externally tagged objects, case, whitespace, BOM and unknown
strings. Node 24 and 26 pass. A live TypeScript BUY-to-BID mutation compiles and
fails `GRVT complete variant inventory` (exit 101). Replacing GrvtOrderSide's
string front gate with derive Deserialize compiles and fails
`GRVT string-only accepted-set refusal` on a tagged object (exit 101). Both
restored, green again.

Host all-target strict Clippy and separate wasm all-target strict Clippy pass.
Scoped all-target tests including ignored and fmt pass. Cargo.lock adds only
serde/serde_json references to this crate, with no package or version changes;
both use existing workspace dependencies/features. No production helper API or
exemption was introduced. These six mappings leave 152 pending rows.

The reviewer identified an extra-Rust-spelling coverage gap. Private wire enums
now retain Deserialize variant metadata; the oracle compares their complete
accepted sets to the TypeScript checker. A new EXTRA alias mutation compiles
and fails `GRVT complete variant inventory` (exit 101). The derive control
also removes its now-unused private companion so it reaches the named runtime
assertion rather than failing compilation under dead-code warnings.

Final sub-agent delta review: MERGE after the metadata repair. Global parity
passes 6,984 rows / 1,281 public items; extractor remains zero drift; all 190
CI/layer tests pass. No omitted/masked test targets.

## Full-format protocol records

Twelve record declarations are decoded through shared strict_decode::object and
private remote companions (no public inherent bypass). The actual TypeScript
checker judges 934 probes: 489 accepted in TS, including 180 lone-surrogate
representation refusals in Rust; 445 rejected in both. The 309 common positives
retain declared fields on roundtrip, including optional omission, Unicode scalar
strings, astral pairs and whitespace. Each divergent row cites the canonical
shared-utf8-string-boundary declaration as CHOSEN. No replacement characters.

Three independent compiling mutations exit 101 at GRVT record accepted set:
TS client_order_id optional-to-required; Rust positional-array ingress by
removing the map guard; and Rust null widening by removing the optional-present
decoder on the private WireGrvtTicker companion. Baseline restored after each.
The null control must touch the private decode companion: removing the attribute
from the public Serialize-only struct does not change deserialization.

Node 24 and 26 pass. Host strict Clippy and separate wasm strict Clippy both
pass, without --no-deps. Scoped native tests, two compile-fail doc tests, fmt,
wasm build, extractor zero drift and global parity (6,984 / 1,293 items) pass.
The existing GRVT root gate now also runs --doc, pinned by the CI audit. Lock
changes only add this crate's shared-types dependency reference. Twelve mapped
CHOSEN rows leave 140 pending, with twelve ported rows unchanged.

Reviewer coverage suggestions applied: all enum choices are now exercised in
record fields, and nested array elements receive missing/null/wrong-type probes.
A fourth compiling control weakens bids to maps of strings, accepting a missing
required nested field; it fails GRVT record accepted set at exit 101. Restored
the typed nested record and baseline passes again.

## Integration merge-down: 91db928805

Merged into venue head 53f41afa12 with no textual conflicts and no stash. Named
CI steps: 74 / 76 on parents, 77 merged; zero missing from either parent. Real
tomllib parse: 301 exemption BLOCKS, 301 DISTINCT targets, zero blank reasons;
all parent targets retained. Thirteen nonempty direct dependency lists on each
parent checked, none emptied. GRVT dependency entries and doc gate retained.

Merged-tree gates: root venue oracle (three live tests and two compile-fail doc
tests), host all-target strict Clippy, separate wasm all-target strict Clippy
without --no-deps, scoped fmt, 197 CI/layer tests, locked offline metadata,
extractor zero changed/added/orphaned and global parity (6,984 / 1,451 items)
all pass. Counts remain 12 ported / 12 divergent / 140 pending.

Independent sub-agent MERGE review inspected the complete prospective merged
tree in its own checkout against both parents. It independently confirmed the
step counts, dependency retention, script preservation, parsed exemption union
and unchanged GRVT code/manifest counts. No merge findings. The unfinished
balance-prototype witness is untracked and excluded from this merge.

## Fill record and integration 8cbfb8b5f9

GrvtFill and its inline MAKER/TAKER liquidity union extend the actual-source
checker corpus to 1,041 records: 544 accepted by TypeScript, including 200
lone-surrogate representation refusals, and 497 rejected by both. Seven unions
cover 37 literal positives and 629 refusal probes. Node 24 and 26 pass. Independent
TS and Rust liquidity spelling mutations compile and fail the named complete
variant inventory assertion; both restored. No numeric ingress conversion ships.
The rejected InstrumentLite candidate lost lexical -0 on bd, qd and fi (source
bits 8000000000000000, decoded 0000000000000000), so that row remains pending
on shared ingress preserving both overflow and signed zero. The twelve previously
shipped records contain no numeric fields. No GRVT source calls v or vn.

Merge-down preserved all 77 / 80 parent CI steps in 80 merged steps. Real tomllib
parse gives 303 exemption BLOCKS and 303 DISTINCT targets with nonblank reasons,
all parent targets retained. All 22 / 26 nonempty direct dependency tables on the
parents remain nonempty. Scoped native tests, two compile-fail docs, host strict
Clippy and separate wasm strict Clippy pass (neither uses --no-deps). Extractor
reports zero changes/additions/orphans; parity passes 6,984 rows / 1,765 public
items; 287 wiring/layer tests pass and the layer audit passes. Counts after this
slice: 12 ported / 13 divergent / 139 pending.

Independent merged-state sub-agent review returned MERGE for staged tree
02b6021dd9a7621f8b682a4cfbeebd4121d7f20a. It reproduced both corpus counts,
confirmed all thirteen records have no numeric fields, and checked parent CI
steps, scripts, dependencies, lock references and all exemption containers.

## Held fetchBalance source witness

The mandatory all-target gate now executes the actual TypeScript fetchBalance
member in thirteen fresh VM realms. USDT inserts normally; the eleven inherited
callables mutate their own free/total fields to NaN without an own currency key;
__proto__ instead mutates Object.prototype globally, observed through fresh
empty and unrelated objects. Prototype identity stays unchanged and the lost
currencies serialize as {}. REST is injected at the real post boundary; tu=0
avoids Store access, enforced with a throwing Store getter. No fromEntries or
local setter replica is used. Node 24 and 26 pass. Changing the actual source
guard to Object.hasOwn compiles then fails the named inherited-currency
assertion (exit101); restored baseline passes. Host Clippy, parity and extractor
drift pass. The row remains pending with a FINDING and ts-bug marker; this adds
evidence only, not a Rust mapper or a completed row.

Independent sub-agent review MERGE: reproduced all thirteen actual-member cases
and the own-guard mutation failure; confirmed fresh intrinsics and in-realm
fixtures, enforced Store isolation and unchanged pending status.

## Integration merge-down: b8fc4dfbaa

Independent sub-agent MERGE for tree69e38d0a8de9e8313c8eaf67a82328cbf1e3e01a.
Both parents retain all steps (80/82 to82), scripts, dependency lists and local
lock references;312 exemption BLOCKS/312 DISTINCT, nonblank and correctly
contained. GRVT code and manifests unchanged from3210c761f0. Scoped tests,
host and separate wasm strict Clippy pass;295 wiring/layer tests pass; extractor
zero drift; parity7,334rows/2,172items passes.

## Remaining string/bool protocol records

OrderLeg and GrvtBuilderAuthorizationResult use the same shared map-only ingress
and private codecs. The oracle appends their exact AST interface declarations
from the actual signing/builder source files; no imported dependency is stubbed
or executed for these standalone types. Corpus1,093:567 TS accepted including
208 lone-surrogate refusals;526 rejected by both. Node24/26 corpus outputs match.
TS limitPrice optional-to-required and Rust camelCase-decode removal controls
both compile then fail named GRVT record accepted-set assertions; restored
baseline passes. Scoped tests, host Clippy and separate wasm Clippy pass;
extractor zero drift and parity7,334rows/2,174items pass. Both rows cite CHOSEN
shared UTF8 representation; no numeric ingress claim. Counts12ported/15divergent/
137pending.

## InstrumentLite numeric record (depth 2)

GrvtInstrumentLite now captures shared JsNumberInput in its public Deserialize,
then uses a private map-only codec. The measured production API is that decoder
(bare raw/owned/borrowed Serde plus the three shared entries), not fetchMarkets,
which remains pending. All three numbers bd/qd/fi pass 642 live JSON.parse field
cases and 3,852 bit comparisons on Node24 and26, including integer -0, float -0,
overflow, negative underflow and binary64 rounding. The old pre-Value-first
candidate is a separate fixture, not a shipped regression guard. Reproducing its
extra Value::deserialize buffer loses integer -0; bare from_value alone does not.

Sixteen records now have 1,265 live TypeScript checker cases, including 239
lone-surrogate representation refusals and a nested optional string-array witness.
TS bd:number-to-boolean mutation fails a named accepted-set assertion at exit101.
Rust production bd zero-normalization compiles and fails the named numeric bit
assertion on bare raw ingress (path0); restored source passes. No ignored tests.
Native tests and doctests, host strict workspace Clippy and separate wasm strict
workspace Clippy pass. Parity passes7,334rows/2,535items; extractor zero drift
using the disclosed isolated compiler install. Only GRVT's dependency list and
its lock entry add the already-present shared num crate; no package/version edit.

Integration82445b433d fast-forward retained87/87 named CI steps. A real tomllib
parse gives327 exemption BLOCKS and327 DISTINCT, zero blank reasons. Source and
merge adversarial sub-agent review returned MERGE. Counts12ported/16divergent/
136pending. The currency inherited-referent mutation remains held and no global
write was reproduced in Rust.

## Signing and builder numeric records

GrvtSignature, OrderPayload, GrvtBuilderCheckResult and GrvtBuilderInfo use
public depth-2 capture and private map-only codecs. All declared numeric fields
are f64, including integer-described signature/fee fields. Signing and builder
runtime methods remain pending. The actual interface checker now covers1,439
cases with259 shared UTF-8 refusals; the numeric inventory checks all8 fields
across5 records against214 spellings each, yielding1,712 cases and10,272 bit
comparisons per Node24/26 run through6 bare/shared ingress routes.

TS signature-v number-to-boolean mutation assertion-fails by name; Rust public
signature-v zero-normalization compiles then fails bare raw path0 at v:-0.
Restored full all-target tests pass on both Node versions; host and separate wasm
strict workspace Clippy pass. Parity passes7,334rows/2,539items. Independent
source sub-agent review MERGE, no findings. Counts12ported/20divergent/132pending.

Store-notification sweep: zero already-ported GRVT rows cross Store::update or
set_setting. The12 rows are5 protocol tables,6 standalone string unions, and the
shared pure transport isGrvtRead predicate. Source contains no Store writes;
future runtime/store paths remain pending and must account for subscriber errors.

## Notification reachability re-audit

The twelve ported rows have no reachable Store notification: five immutable
protocol tables and six string unions have no call graph. The sole callable,
TypeScript isGrvtRead, reads config.url and calls lastSegment (String.split twice
and Array.pop) then Set.has over the static endpoint set. Rust is_grvt_read uses
String split/rsplit/next and matches literal endpoints. Neither invokes retry
execution, send/reconnect, health publication, snapshot helpers or a Store API.
The caller createAPI remains pending: incoming callers do not become outgoing
edges of this predicate. This is a reachability result, not a direct-grep zero.
Bybit's STORE_NOTIFICATION_AUDIT.md on its published branch supplied the method
correction; no GRVT row needs reclassification from this sweep.

## WebSocket wire records

GrvtWebSocketSubscription and GrvtWebSocketMessage, their nested params/error
records and two literal unions are public depth-2 decoders. The runtime remains
pending. The live checker covers1,554 record probes including264 deliberate
UTF-8 representation refusals; nine unions cover40 literals and680 refusals.
Opaque params/result/error.data retain shared JsNumberInput and distinguish
absence from present null. A source-checker witness accepts executable function
and Symbol payloads, records typeof before JSON serialization omits them, and
pins the declared CHOSEN JSON-wire domain instead of claiming all TS any values.

Numeric tests cover2,140 source field/spelling rows through6 entry routes:
12,840 bit comparisons plus3,852 opaque payload comparisons per Node24/26 run.
The actual TS JSON-RPC literal mutation fails the named full-inventory assertion.
A Rust production subscription-id zero-normalization mutation compiled and failed
the bare-raw path0 bit assertion at -0; restored source passes. All-target tests
and doctests pass; host strict Clippy and wasm strict Clippy separately pass.
Fmt and315 CI-wiring/layer tests pass. Parity passes7,334rows/2,545items; the
isolated extractor reports zero drift. TOML parses as327 exemption BLOCKS and327
DISTINCT. Source review returned MERGE without findings. Counts12ported/22divergent/
130pending. No integration merge or socket-runtime completion is claimed.

## Consolidation through worker integration 1f6698344c

Merged aa632b5578, eefa54442a and1f6698344c in reviewed stages. Final CI steps
88/88->88; real tomllib parse333 BLOCKS/333 DISTINCT, no blank reasons or parent
loss. Each merged-state review returned MERGE. Full GRVT all-target tests and
num numeric-ingress/library tests plus separate host/wasm strict Clippy passed
on the eefa stage. The last worker stage changes no GRVT/num/types source,
manifests, dependency declarations or lock entries; their scoped results carry
forward unchanged. Rerun global parity passes7,334rows/2,557items,319 CI/layer
tests pass, and extractor reports zero drift. GRVT counts12/22/130 remain.
Shared container eligibility f1a59dd62f is included; its own proof and conservative
container census are in num/tests/CONTAINER_ELIGIBILITY.md.

## Builder protocol declarations

Four string constants execute from actual grvt.builder.ts AST declarations,
including the legacy mainnet alias. GrvtBuilderUnauthorizedReason adds two
string-only literals; the complete union corpus is now10 unions/42 literals/
714 refusal probes. Uppercase mutations of lowercase literals are included.
TS address and reason mutations fail named assertions in child tests; a Rust
mainnet address mutation compiles then fails the named constant assertion. All
mutations restored. Source sub-agent review MERGE. Full GRVT all-target tests
pass on Node26; changed builder/type oracles also pass Node24. Host strict Clippy
PASS, wasm strict Clippy separately PASS, fmt PASS, extractor zero drift and
parity7,334rows/2,562publicitems. No dependency/CI wiring changes; existing
all-target venue gate discovers builder_protocol automatically. Counts17/22/125.

The5 new ported rows are immutable strings or a closed union and add no outgoing
call graph, so the prior notification reachability audit remains zero across17
ported rows. Builder authorization, fee conversion, error class and signing
remain pending.

## Trigger metadata record

GrvtTriggerMetadata uses public depth-2 capture, local string-only GrvtTriggerKind
and shared OrderStopTriggerType. Both the actual metadata alias and actual
shared enum declaration feed the TypeScript checker; enum-member expressions
bridge only recognized wire strings, leaving invalid strings as refusal probes.
There are41 trigger cases (10 accepted,31 refused), for1,595 total record cases
and264 UTF-8 refusals. Eleven unions cover44 literals and748 refusal probes.
The numeric corpus now has2,354 rows; triggerPrice contributes214 spellings
through6 bare/shared paths (1,284 bit comparisons). Its deriving helper remains
pending: the measured production caller is the public Deserialize entry.

Actual TS triggerPrice number-to-boolean mutation fails its named record
assertion. Rust production signed-zero normalization compiles then fails named
bare path0 bits, and is restored. Source review MERGE. Full GRVT tests pass on
Node26 and affected record/numeric/union tests on Node24. Host strict Clippy and
wasm strict Clippy separately pass, fmt passes, extractor zero drift and parity
7,334rows/2,564items. Counts18ported/22divergent/124pending. The new record decoder
adds no reachable Store notification; no runtime mapper or socket is claimed.


## Builder address and fee helpers

Actual AST-extracted getGrvtBuilderAddress and grvtBuilderFeeRateToRaw execute
with native VM built-ins. 10,113 fee strings compare bit patterns or exact Error
messages; both boolean networks compare addresses. Cases include decimal prefix
parsing, exponent/underflow/overflow, negative ties and signed zero, and finite
inputs whose multiplication overflows. Three double-escaped lone-surrogate inputs
retain the source output/error code units alongside Rust's representation refusal.
No uint32 narrowing or post-multiplication finiteness guard is introduced.

Independent source mutations replace Math.round with Math.floor and invert the
network condition. Independent Rust mutations use truncation and invert the
network selection. All compile and fail named fee-bit/address assertions; Rust
production restored afterward. The helper rows do not claim their authorization
or signing callers. Store reachability remains zero: these call only shared
parse_float/math_round or select immutable constants.

Validation: helper suite passed on Node 24.20.0 and 26.7.0; full scoped GRVT tests
passed on Node 26. Host strict Clippy PASS and wasm32 strict Clippy PASS separately;
formatting, zero extractor drift and parity (7,334 rows / 3,213 public items) PASS.
The exact corpus sizes are pinned. Counts after this slice: 19 ported, 23 divergent,
122 pending. No batch mapper or eager typed-output refusal is introduced.

## Builder info construction

The actual createGrvtBuilderInfo declaration executes with its actual dependency
functions. 324 combinations cover omitted/false/true network, nine address forms
and twelve fee forms; two additional whole-function calls exercise all 1,112,064
Unicode scalars in isolated and contextual lowercase positions. Tests compare
address, original REST fee and signing Number bits or exact Error message before
serialization. Three double-escaped lone-surrogate address witnesses retain the
source output code units alongside the Rust ingress refusal.

Source mutation is applied to the selected function declaration, not the first
matching lowercase expression elsewhere in the module. The initial whole-file
mutation missed this function and was rejected as an ineffective control. The
corrected source mutation and independent production Rust lowercase bypass both
compile and fail the named address assertion; production is restored and green.

Full GRVT tests pass on Node 26.7.0; the new suite passes on Node 24.20.0. Host
strict Clippy PASS and wasm strict Clippy PASS separately, formatting and layer
audit PASS, extractor zero drift (isolated local tsx install), parity PASS at
7,334 rows / 3,234 items. Real tomllib: 338 exemption blocks / 338 distinct targets,
no blank reasons. Counts: 19 ported / 24 divergent / 121 pending. This whole helper
has no Store or async path and no multi-row refusal; browser authorization and
order signing callers are not claimed. The only divergence is the canonical
UTF-8 boundary. The latest integration d0b7d30879 is included via ce59e69936.

## Builder unauthorized error

Actual source class construction supplies 1,632 combinations: both reasons,
16 required Number values, absent plus16 authorized cap values, and absent/empty/
Unicode override messages. Both caps compare bits before JSON; name, code,
reason, message and Error.toString compare independently. The initial native
Display included a colon for an empty message; the source witness failed that
named assertion and the corrected display now matches. Three surrogate-message
witnesses retain valid JSON escapes and source code units. Source stack presence
and Error prototype are measured but intentionally absent from the native type;
this chosen boundary is recorded separately from UTF-8 in both manifest rows.

Source nullish-to-OR mutation and production Rust zero-cap filtering both compile
and fail the named cap-message assertion; production restored. Full GRVT suite
passes on Node26, affected suite passes on Node24; host strict Clippy PASS and
wasm strict Clippy PASS separately. Parity7334/3236, extractor zero drift, fmt and
diff checks pass. Review is against the complete constructor slice. Counts now
19 ported /26 divergent /119 pending. No Store or batch path is added.


## Order semantics and inherited-field census

Actual source functions and shared enums supply 14,400 TIF cases and 10,998
trigger cases. Full/lite aliases at every nesting level, null versus missing,
strict booleans, Number coercion, overflow, underflow and signed-zero bits are
compared. Function/Symbol typeof outcomes and escaped-surrogate witnesses retain
the separately declared non-JSON and UTF-8 boundaries. These pure helpers add
no batch refusal, callback or Store notification path.

Source OR/parseFloat mutations and Rust ignored-postOnly/zero-normalization
mutations compile and fail named assertions, then production is restored.
Full Node26 tests, affected Node24 tests, host strict Clippy and wasm strict
Clippy separately passed before the pause; parity 7334/3238. serde_json moves
from dev to production dependency with zero resolved package/version/feature
delta and no lock change. Four inherited-required-number controls pin named
refusal in the numeric and interface-derived union inventories.

## SDK builder address checksum casing

The actual vendored `Signer.sign` delegates to MetaMask V4, which encodes the
SDK's 20-byte builder address without ethers' EIP-55 checksum admission. The
shared native EIP-712 preflight previously rejected a mixed-case address which
that signer accepts. The SDK adapter now lowercases only a valid 20-byte,
0x-prefixed builder in its local OrderWithBuilderFee signing copy; the caller's
original spelling is retained. Shared crypto and legacy signing are unchanged.

The maintained actual-SDK oracle expands all eight network/order/integer
configurations across lowercase, uppercase and bad-checksum mixed-case addresses.
All 24 source signatures equal their corresponding lowercase signatures. Before
repair, the native oracle test compiled and failed `GRVT SDK builder casing` on
the mixed-case input. After repair, every native signature and decoded r/s/v
matches the actual SDK, and native caller-field checks prove no input mutation.

All 22 library tests (including the ignored SDK/schema/cookie witnesses) and 21
constructed exchange tests pass. Fresh compiler inventory retains all nineteen
reviewed aliases. Full native and actual target-specific Wasm stored-contract
runs each pass 134 receipts (117 runtime, seventeen explicit classifications),
and all 268 pass the scoped ratchet. Strict host all-target/all-feature and Wasm
library all-feature Clippy, crate formatting and parity checks pass (7,459 rows,
17,214 public Rust items). This is a casing repair for the named SDK address
shape, not broader MetaMask/ethers encoding equivalence or whole-crate clearance.
