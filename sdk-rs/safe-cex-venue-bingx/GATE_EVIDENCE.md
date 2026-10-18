# BingX constants slice — 2026-10-13

Integration `a3d514bec82d6760d4e9e7a27186f992d175c36d` was merged down as
`d860470721e499745e61822e90ef611e386122e6` before implementation.
Python 3.12 `tomllib` over the five BingX manifests measured the starting
ported/divergent/pending triple as **0/0/99**, and this slice as **12/0/87**.
The twelve claims are configuration/mapping declarations, not their consumers.

All Cargo commands used `/opt/homebrew/opt/rustup/bin/cargo`, the SDK manifest,
`-p safe-cex-venue-bingx`, and `--jobs 1` (fmt has no jobs option). No Yarn shim
was invoked. Live tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

Measured gates:

- `cargo check`: exit 0.
- `cargo test --all-targets --locked`: 2 passed, 0 failed, 0 ignored.
- `cargo fmt --check`: exit 0.
- Host `cargo clippy --all-targets --locked -- -D warnings`: exit 0.
- Separate `cargo clippy --all-targets --target wasm32-unknown-unknown --locked -- -D warnings`: exit 0.
- `python3.12 tests/parity_scope.py`: real parity checker passed 99 rows and
  12 public Rust items. The temporary index workspace contains only BingX;
  dependency paths still point to the landed shared crates. No exemptions.
- `python3.12 tests/constants_controls.py`: 16 mutations failed at named
  assertions and each restored state passed its selected test.

The controls cover each of the twelve declarations (native `RECV_WINDOW` bits,
then `ordered constant <name>`), a same-typed BUY/SELL value swap
(`ordered constant ORDER_SIDE`), inverse collision reordering
(`inverse order ORDER_TYPE`), an actual TypeScript endpoint mutation
(`ordered constant ENDPOINTS`), and changing the actual TypeScript table's
prototype (`inherited lookup ORDER_SIDE/constructor`). Sources were restored.
The oracle runs the maintained TypeScript modules; no shared fixture is changed.

False-green scope: arrays retain key/order identity even where source values
are equal; the value-swap control is also independently exercised. There is no
wire decoder, AST field census, numeric ingress or NaN comparison in this slice;
roundtrip symmetry, inherited-field census and NaN canonicalization are therefore
not claimed as covered. Future slices must add those witnesses and revisit these
rows where consumers introduce new behavior. Inverse reconstruction in the Rust
test proves declaration compatibility with actual `inverseObj`, not a production
venue inverse function.

Sub-agent review found a stale-binary risk in the scoped gate runner. It now
pins its target directory and executes the single executable reported by Cargo's
JSON build result. The sub-agent inspected that repair and the sixteen-control
log. Final SHA review is reported in the handoff.

At the constants-slice handoff, not implemented: 7 type rows, 3 API rows, 47 exchange rows, 19 private WebSocket
rows and 11 public WebSocket rows. No integration merge or PR creation.

Load-policy followup: the complete corpus is cached by OnceLock, reducing Node
launches for the two-test target from two to one (there was no per-case loop).
The two-test constants target passed again; the actual-source mutation failed
`ordered constant ENDPOINTS` and restored/passed again. Host and separate wasm
strict clippy and fmt passed after this change. The control runner accepts
`--only '<label>'` for targeted rechecks and rejects an unknown label. The full
sixteen-control evidence above belongs to the preceding constants implementation;
this followup changes only test capture scheduling and control selection.


# BingX types slice — 2026-10-13

Implementation commit: `d8c69280c8`. Integration
`f820788e5a2430b23505a40746f5c1cd33af4df7` was merged down at
`2b296490c562bf307964af80dd7e87b13ab94945`.
Real tomllib measures **13 ported / 7 divergent / 79 pending**, total 99.
Seven response records cite the canonical CHOSEN UTF-8 string boundary;
the closed order-status union is ported. Remaining rows: API 2, exchange 47,
private WebSocket 19, public WebSocket 11.

The actual TypeScript checker supplies the source field census, including
recursive heritage validation. Maintained TypeScript modules execute in the
oracle. The type-only API alias is a declared-shape witness, not a claim about
unchecked Axios runtime values. One Node invocation supplies each whole corpus.
The types target measures 631 typed cases through five decode routes and
252 UTF-8 boundary witnesses through two raw-input routes. Native fields and
actual TypeScript JSON.stringify wire output are checked independently.

Eight production mutation controls each failed a named assertion and restored
and passed: BINGX_BALANCE, FETCH_BALANCE_RESPONSE, FETCH_LEVERAGE_RESPONSE,
FETCH_HEDGE_MODE_RESPONSE and FETCH_POSITIONS_RESPONSE exercised type decode
or native-field assertions; BINGX_ORDER_STATUS and FETCH_MARGIN_MODE_RESPONSE
exercised Serialize-only wire mutations; ORDER_TRADE_UPDATE exercised absent
optional-field wire shape. The leverage swap failed
`native_fields FETCH_LEVERAGE_RESPONSE`. Three oracle controls separately
failed `inherited_field_census`, `unresolved_heritage_refusal` and
`transitive_heritage_refusal`, then restored and passed. The inherited control
was narrowed to Child after recursive base validation so it still fails its
own named assertion.

Measured before this evidence commit: merged-state host and separate wasm
strict clippy passed; the scoped parity index passed 99 rows and 28 public
Rust items before the integration-only CI merge. Sub-agent review reported
zero remaining findings on that merged state plus the inspected census-control
adjustment. Those measurements are historical, not relabeled as this commit.
Final exact-SHA gates and review are reported externally after committing.
Origin remains pinned at `683fbb813137ab9acaadda503a8fd338b50e773e` under the
fleet readiness freeze; this types slice is local pending branch release.


## Integration refresh and release authorization

The subsequent explicit push instruction releases the prior local-only hold.
Integration `5b3f63f2ea` is merged at
`385df6a4d8fc5241dcdfab5178811af47bd8f4b5`. The types oracle now records Node
version, architecture and platform in its corpus and prints them in the test
output. Measured runtime: Node v26.7.0, darwin arm64. Numeric native observations
canonicalize only NaN; signed zero and all other numbers remain bit-exact.
Distinct numeric samples and path-specific string samples prevent equal-valued
field pairs from masking a symmetric rename; decoded native fields are asserted
independently of Serde metadata. The existing production mutations remain the
historical per-row evidence. Final gates and review attach to the exact committed
refresh SHA in the handoff; the branch freezes again after that push.


## Local utility slice: API sort

Origin remains frozen at reviewed `4de4000557f0912dbf224189530aaa01f43dab5b`.
The next local slice maps only the `sort` helper: tomllib **13/8/78**, total 99.
`api::sort` delegates to shared JsRootCollator. Its CHOSEN row documents both
the canonical UTF-8 boundary and root/English ordering instead of the host's
locale. The API factory and its consumer behavior are still pending.

The actual TS initializer is extracted by the TS parser, transpiled and executed
without replacing its body. One Node corpus yields 784 key-pair comparisons and
three lone-surrogate acceptance observations paired with Rust UTF-8 refusal.
Measured capture: Node v26.7.0, darwin arm64, ICU 78.3, default locale en-US.
Comparator results are checked as native Ordering. Reversing production arguments
failed `native sort ordering` with exit 101 and restoration passed. Reversing the
actual maintained TS comparator separately failed the same named assertion with
exit 101 and restoration passed. No TS changes remain.

Final scoped gates and sub-agent verdict will be measured on the committed local
SHA and reported outside this file; this text does not pre-claim their results.


## Requested integration refresh: 474b08e6e32b

Integration `474b08e6e32b` was merged at
`fd4336393bc2440e54419c08d7d18e35ff7c596b` following the explicit refresh/push
instruction. Real tomllib remains **13 ported / 8 divergent / 78 pending**.
BingX double-parse behavior is **UNMEASURED**: the transport/API parse chain is
not covered by the implemented declaration and comparator rows. No conclusion
about presence or absence of that defect is claimed. The final scoped sheet and
sub-agent review will attach to the committed refresh SHA in the handoff.


## Local TradFi error utilities

Reviewed remote remains ed82bf3301c90ac6df0aadd7094665bc6dae26ec (13/8/78).
This local-only slice maps buildTradfiOneWayModeError and
normalizeOrderPlacementError under the canonical CHOSEN UTF-8 boundary.
Actual tomllib is **13/10/76**, total 99. The source arrow initializers execute
with their actual helper-to-helper lexical-this call, without replacement
implementation. One Node process returns 70 builder cases, 65 normalizer cases,
and two surrogate-retention/boundary witnesses. Runtime is Node v26.7.0,
darwin arm64. Direct output comparisons include mixed-case substring matches,
empty/absent details, and nonmatching Unicode case variants.

Each production helper mutation failed its named native assertion (exit 101)
and restored/passed: native buildTradfiOneWayModeError and
native normalizeOrderPlacementError. An earlier unused-variable mutation was
rejected at compile time, was not counted as evidence, and was replaced with
the assertion-failing builder mutation. Source restoration uses finally.

AUDITED ZERO Store inventory remains applicable: these are string helpers,
not Store or transport error conversion. Their manifest notes retain future
notification-identity propagation requirements and Store-LOCAL identifiers.
Correction: the earlier claim that ExchangeError.store_subscriber_ids had landed
was false; it is absent from integration 9419b340d8a6. Double-parse
behavior remains UNMEASURED. Exact-commit gates and review follow this commit;
origin is not advanced by this local follow-on slice.


## ABI broadcast correction (local documentation only)

Verified against canonical 9419b340d8a6 with git grep: no store_subscriber_ids
symbol exists under sdk-rs. git merge-base --is-ancestor ffe81b5652 9419b340d8a6
returned 1. The conversion at safe-cex-exchange/src/runtime.rs:81 clones the
original ExchangeError when it can downcast it, otherwise projects the message
and available response metadata. It does not structurally retain the notification
wrapper or subscriber id. Subscriber-id structural retention is a named shared-core
FINDING, not an implemented API. BingX has no consumer or migration to perform;
its AUDITED ZERO remains. No row status changed, no sidecar was introduced, and
origin remains pinned at ed82bf3301c90ac6df0aadd7094665bc6dae26ec (13/8/78).


## Subscriber identity closure (local documentation only)

Fetched integration 9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7 and inspected runtime.rs directly:
ExchangeErrorMetadata defines store_subscriber_ids: Vec<u64>, the recovered
conversion appends notification.subscriber_id() to the cloned list, and the
fallback conversion appends it to the projected error. Ordering is inner-to-outer;
ids remain Store-local. The historical absence at 9419b340d8a6 above is unchanged,
but the named shared-core structural-retention FINDING is now CLOSED at the fetched
SHA. Both local utility rows record the closure. BingX has zero Store consumers,
zero Store dependencies and no identity-preserving box or sidecar to simplify.
No production code or row statuses changed. No integration merge or push was made;
reviewed origin remains ed82bf3301c90ac6df0aadd7094665bc6dae26ec.


## Utility branch handoff

Separate branch feat/rust-venue-bingx-utilities includes freshly fetched
integration 710e663156c8a3c43f8d40af2aa2076b960368d8. The original slice was
merged by PR7246; the formatting-only fix remains separately queued in PR7261.
This slice adds two error-message utilities: tomllib **13/10/76** (99 total).
All ten divergent rows lead with CHOSEN and cite the canonical UTF-8 rationale;
the comparator also retains its independent locale rationale. Real extractFile
and reconcile on the changed exchange manifest report zero added, zero orphaned,
and zero changed after canonicalization, with tomllib semantic equality.

Both production utility mutations were remeasured after the integration merge:
native buildTradfiOneWayModeError and native normalizeOrderPlacementError each
compiled, failed at exit101, and restored to one passing target. Each corpus
uses one Node invocation. Final exact-SHA scoped gates and sub-agent verdict
follow this commit and are reported in the handoff. Precision remains a negative
census (zero sites, zero fields swapped), with per-field controls required for
future formatter rows. Double-parse remains UNMEASURED.


## Synthetic ticker utility after MEXC integration repair

Integration e53d89eddeef1fc1dddf919fed994d0c8b9df48b was fetched and merged
at bd1cdf6745a5607dca803fa3e55d859e0a2c4987 on a follow-on branch, leaving
reviewed utilities head57d8943e9d unchanged. BingX scoped cargo check passed;
no workspace-build or MEXC test claim is made. This slice adds deriveTickerBidAsk:
actual tomllib **14/10/75**, 99 total. Existing ten divergences retain CHOSEN
canonical UTF-8 rationale plus the comparator's locale rationale.

The live witness executes the source arrow with actual safe-math and isNotNumber
modules. 168 cases compare native bid and ask bits before JSON projection;
NaN alone is canonicalized, signed zero stays exact. Two independent production
swaps (bid and ask price-step operands to amount-step operands) compiled and failed
exit101 at their respective BINGX-PRECISION native assertions, then restored and
passed. Original corpus size/hash and per-field classification are recorded in
PRECISION_SWAP_AUDIT.md. No fixture-value changes repaired these controls.
Final exact-SHA gates and review follow the commit; double-parse remains UNMEASURED.

## Market utility slice

Base: fetched integration 3e9a26473539bd4f67f700edd6f26a1306cf07f5.
Real tomllib counts before: 14 ported / 10 divergent / 75 pending; after:
14 ported / 12 divergent / 73 pending (99 rows). Both new rows cite the shared
UTF-8 boundary as CHOSEN; no unmeasured transport behavior is classified.

`market_utils_match_actual_typescript` executes the exact source initializers
in one Node invocation for 75 classification and 138 symbol resolution cases.
Native outputs are compared directly. Cases cover index precedence even for
empty IDs, all 12 standard Object.prototype names, own prototype-name shadowing,
duplicate keys, mixed case, Unicode scalars and trailing line terminators.
The capture records Node version, architecture and platform. Lone-surrogate
behavior is observed in TypeScript and paired with Rust UTF-8 refusal.

`market_utils_controls.py` runs four production controls (classification,
reversed index precedence, empty-ID fallthrough and inherited-hit fallthrough)
and twelve corpus controls: empty/subset/extra per helper, each with source
admission guards intact and bypassed. Named failures distinguish source totals,
executed comparisons and native outputs; each control restores source bytes and
requires the single target to pass. These scalar helpers have zero production
output collections and zero precision adjustments, so these controls are not
claims about consuming array deserializers or additional price/amount fields.
Double-parse remains UNMEASURED. Final SHA gates and review are sent separately.

## Precision-string utility

Base fetched from origin: dbdf3d15dddc29d9ae885398c62fd0fe2a25acbc.
Tomllib before 14/12/73, after 14/13/72 (ported/divergent/pending, 99 rows).
The one new divergence cites the canonical UTF-8 boundary. Actual maintained
`_parsePrecision` method parameters and body execute in one Node invocation:
27 strings and 18 numeric-bit inputs, plus two lone-surrogate observations.
Its zero/negative/NaN results and inferred hexadecimal radix are source behavior,
not normalized decimal precision. Huge positive values are not exercised; this
literal source loop remains resource-bound rather than silently clamped.

Two production controls alter the loop bound and inferred radix; each must
compile and fail BINGX-PARSE native precision, then restore and pass. Six scalar
source-corpus controls empty, shorten, or enlarge the capture, with and without
the source-total guard. The independent executed subcorpus count rejects all
three after admission is bypassed. No production collection is returned, no new
per-field price/amount swap site is introduced, and REST callers remain pending.
Double-parse remains UNMEASURED. Exact-SHA gates and sub-agent review follow.

## Primary balance/account REST reads

Separate branch follows pinned precision parser 4e0300db38; that PR head is not
modified. Tomllib before 14/13/72; after 14/15/70 (99 rows). Two new rows cite the
canonical CHOSEN UTF-8 boundary. The configured callback uses shared RestRequest,
JsNumberInput and ExchangeError; the pending API factory/signing/transform chain
is not bypassed and claimed as implemented. Double-parse remains UNMEASURED.

The source witness executes both maintained methods together, actual ENDPOINTS,
and only substitutes the xhr network boundary: 34 cases per method, plus two
surrogate observations, in one Node process with Node/architecture recorded.
Cases cover first USDT versus first row, duplicate settlement rows, empty/missing
responses, strict code typing, errors, String(userId) coercion and shadowed
non-callable toString. The latter revealed an Error/TypeError mapping mistake in
pre-commit sub-agent review; native name/message assertions cover the repair.
Full native request equality observes all overrides, not only URL/method.

Permanent controls mutate selection, account field, error propagation, request
configuration, fully consumed response arrays (empty/subset/extra winning row),
and native row objects (empty/subset/extra). Scalar source corpus empty/subset/
extra controls run with admission guards present and removed. This is a
post-consumption response mutation, not an early-return serde sequence mutant.
No new price/amount precision consumer is introduced. There are no Store writes;
shared error metadata passes through without a venue-local identity sidecar.

Numeric observation supplement: the original 68-case capture is preserved in
`tests/fixtures/account-reads-original.json.gz` (evidence only, never expected
outputs for the live test): 22616 bytes, SHA256
`a945e7366e04936ce9791603a636bb6d9e3d500e0659f1dab0996005f3d988f6`.
Ten additive raw-input cases cover -0, 0, 9007199254740993, 1e400, and -1e-400
through both source methods. Primary balance userId bits are observed before
JSON serialization; account userId is compared as a native string. The numeric
source total (10), executed counts (5 per method), native field inventories
(9 balance / 1 account) and complete requests are independently asserted.
Original 68 inputs and observations are unchanged. Two additional production
controls erase negative zero or infinity; six numeric-corpus controls repeat
empty/subset/extra with admission present/bypassed. Total: 25 controls.
No NaN input is included in this supplement; signed zero is bit-exact.

## Executable account-read input content admission

Correction to the handoff interpretation: the archived capture SHA above was
historical evidence, not an assertion run by the live target. The follow-on now
asserts full method/input sequence equality against account-reads-inputs.json,
independently for 68 main inputs, ten raw numeric inputs and two boundary inputs.
It never reads expected outputs from that fixture. The original capture remains
byte-identical; main/boundary fixture projections were checked against it. Raw
numeric JSON strings retain their original lexemes. Runtime metadata is excluded
from the pin, so Node/architecture changes cannot masquerade as input drift.

Six additional controls replace one entry or reverse each whole subcorpus without
changing its length. They must fail the named cases/numeric/boundary input-content
assertion. Existing source-count bypass controls now remove both admission guards,
leaving fixed executed-method counts in place. All 31 controls require a compiled
named exit-101 test failure and a restored pass. No production or manifest change;
tomllib remains 14 ported / 15 divergent / 70 pending. API factory/auth/transform
remains pending and double-parse remains UNMEASURED.

# Data reads slice — 2026-10-14

Implementation and gate input SHA: `5aedc7fc37572d41f02599c24d9da717681c342d`.
Fresh branch `feat/rust-venue-bingx-data` starts directly at integration
`68a96278d606dd4cf2ff6563e9f04ec7b6848448`; no unreviewed feature ancestor.
Python 3.12 `tomllib`, comparing each row by `(ts, kind)` against that parent,
measured **14 ported / 15 divergent / 70 pending** before and
**14 ported / 24 divergent / 61 pending** after: **0 pending→ported,
9 pending→divergent**. All nine have distinct concrete method targets in
`src/data_reads.rs::BingxDataReads`. The class and source constructor stay pending;
this partial receiver only injects the shared BaseExchange and configured HTTP.
The six named helper exemptions cover receiver/capability/anonymous return shapes,
not substitute implementations of pending venue bodies.

## Per-row bucket justification for this slice

These are this slice's own decisions, independent of the ApexOmni slices.
Every row is **CHOSEN**, rather than FORCED: Rust could expose a JavaScript object
runtime, UTF-16 strings and arbitrary throws, but these reads consume the shared
Store, typed models and structured exchange errors. The string restriction is
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`; the closed field
restriction is `sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`.
Matched admitted cases do not erase the differences below or justify `ported`.

| TypeScript row | Concrete method | CHOSEN difference and witness |
| --- | --- | --- |
| `validateAccount` | `validate_account` | An own `balance: undefined` becomes an absent optional field for a nonfinite total. The comparator separately asserts both shapes before comparing other fields. Configured request failures are ExchangeError rather than arbitrary JavaScript throws; error text crosses the shared UTF-8 boundary. Four live cases cover finite admission, invalid total, response-message preference and strict balance-code rejection. |
| `fetchBalance` | `fetch_balance` | Retained `info` and error text use UTF-8. Actual source retains `a\ud800b` in userId; numeric ingress refuses that raw JSON string. Structured transport errors exclude arbitrary primitive throws. Four cases directly observe distinct native total/free/used/upnl bits, including NaN and signed zero. Failure returns the same Arc balance handle, not a copied balance snapshot. |
| `fetchMarkets` | `fetch_markets` | Shared Market base/quote are strings and max leverage is numeric; source accepts truthy string `"75"` as its maximum. Actual source retains that string, while native lowering reports a typed refusal and returns cached markets. All source row evaluations finish before typed lowering: a later null-symbol error wins over the earlier string-maximum refusal, measured against actual source. UTF-8 symbols are the separate shared boundary. Four matching cases retain distinct amount/price precision, strict status, snake aliases, Infinity maximum and falsy leverage fallback. |
| `fetchTickersRaw` | `fetch_tickers_raw` | Opaque daily/price values use numeric-preserving JSON with UTF-8 strings, not arbitrary JavaScript getters, own undefined row fields or primitive throws. Missing top-level data remains distinct from null. Four live cases compare both GET configurations and ordering, returned arrays, transport failure and failure after the first request succeeds. |
| `formatTickers` | `format_tickers` | The shared Store's Market indexes exclude inherited Object.prototype callable hits that source can read through ordinary object indexing; typed Market/Ticker and UTF-8 fields cannot carry the resulting undefined id/symbol objects. Four live cases observe first matching price, duplicate daily order, unknown-market filtering, independent mark/index and volume/quoteVolume values, funding /100 and the pre-guard null daily error. Native numbers are compared by bits before any wire encoding. |
| `fetchTickers` | `fetch_tickers` | Consumes the preceding typed/UTF-8 formatter boundary and updates a Store accepting typed Ticker values rather than arbitrary runtime properties. Four live cases compare results, GETs and publication; additional native tests dispose during the second response and exercise a failing real Store subscriber. ExchangeError::from retains inner subscriber IDs and appends the Store-local outer boundary. |
| `fetchLeverageForSymbol` | `fetch_leverage_for_symbol` | Shared FetchedLeverage requires numeric fields; source stores and returns string `"17"` unchanged. The live boundary witness proves that string result; native refusal reports with the `Failed to fetch leverage` label, returns None and leaves Store leverage untouched. Symbol/error text uses UTF-8; configured failures are ExchangeError, so arbitrary thrown values cannot use source's primitive/JSON.stringify reporting branch. Four matching cases distinguish buy/sell and retain unrelated Store entries. |
| `fetchMarginModeForSymbol` | `fetch_margin_mode_for_symbol` | Symbol/error text uses UTF-8, and failures use structured ExchangeError rather than arbitrary primitive throws. Non-string raw msg is projected to string at the typed BaseExchange reporting edge. Four matching cases prove exact ISOLATED comparison, response-message preference, strict code rejection and report-before-rethrow. The original transport name/message and subscriber IDs survive rethrow. |
| `getOrFetchPositionMode` | `get_or_fetch_position_mode` | The configured transport and reporting edge admit ordinary numeric-preserving JSON, UTF-8 text and structured ExchangeError; arbitrary JavaScript throws/getters and raw non-string msg reporting are outside that edge. Four live sequences distinguish cached return from refetch, numeric zero from string zero, and string `"true"` from boolean true. Each success sequence makes exactly two requests for three calls. |

## Measured witnesses and scope

`data_reads_match_actual_typescript` executes the maintained TypeScript method
initializers and `_parsePrecision` method, along with actual `get-key`, safe-math,
number guards, ENDPOINTS and MarginType/PositionMode declarations. It includes
36 admitted cases (four for each of nine methods), two dynamic numeric-string
boundary results, one later-source-error precedence result, and one lone-surrogate
boundary. All case inputs, method ordering and variants are pinned independently
of outputs; the canonical input fingerprint is `451097675e13b5cf`. Expected
outputs always come from live source execution, never frozen captured results.
Source VM Store/transport capabilities supply controlled inputs; native execution
uses the production shared DefaultStore and BaseExchange. Source/runtime recorded
Node `v26.7.0`, `arm64`, `darwin`. Each executable caches its oracle once.

Additional native tests cover typed field refusals without Store publication,
source-error precedence over typed refusal, in-flight disposal and a failing
Store subscriber. This slice uses the repaired numeric ingress for configured
responses, including explicit `Response.value` passthrough; it does not implement
or claim BingX API response-transform double parsing.

Gates on the implementation SHA above, all Cargo using the explicit rustup Cargo,
SDK manifest, `-p safe-cex-venue-bingx`, `--jobs 1`, and `--locked`:

- `cargo check --all-targets`: exit 0.
- Explicit test targets `data_reads`, `account_reads`, `constants`, `precision`,
  `ticker`, `market_utils`: exit 0; **11 passed, 0 failed, 0 ignored**.
- Crate `cargo fmt --check`: exit 0.
- Real Yarn entrypoint `parity:extract`: exit 0, updated 0 manifests, no added or
  orphaned symbols. `parity:extract:check`: exit 0, would update 0 manifests.

Not run: full BingX suite, workspace check/test, full monorepo suites, host or wasm
clippy, wasm build/test, global parity audit, CI, live exchange/network checks,
API-factory/signing/double-parse checks, lifecycle or websocket gates. No shared
type or shared trait changed. No production credentials or live HTTP were used.

## Compiled family controls and import repair

`python3.12 tests/data_reads_controls.py` exited 0 on the implementation SHA
`5aedc7fc37572d41f02599c24d9da717681c342d`. It stopped after these five controls,
one per new family. Every broken production variant compiled, exited 101 at its
named native assertion, then the restored source exited 0. No survivor:

| Family | Production mutation | Named assertion |
| --- | --- | --- |
| balance/validation | fetchBalance free reads usedMargin | `BINGX-DATA native fetchBalance variant 0` |
| markets | price precision reads quantityPrecision | `BINGX-DATA native fetchMarkets variant 0` |
| raw/format/published tickers | mark reads indexPrice | `BINGX-DATA native formatTickers variant 0` |
| leverage/margin reads | buy leverage reads shortLeverage | `BINGX-DATA native fetchLeverageForSymbol variant 0` |
| account position mode | cache stores OneWay rather than returned mode | `BINGX-DATA native getOrFetchPositionMode variant 0` |

The first scoped parity-index run exited 1: its fail-closed owner resolver treats
a wildcard type import as a possible imported inherent-impl owner. Follow-up
`9d75aa3b919f7ea80e0e148c4dec6470b645b577` replaces only `safe_cex_types::*` with
explicit imports. A Python byte comparison of the entire source suffix beginning
at the first declaration proves every declaration and method body is unchanged;
`git diff --exit-code 5aedc7fc37572d41f02599c24d9da717681c342d
9d75aa3b919f7ea80e0e148c4dec6470b645b577 -- <crate>/tests` also exited 0. The
mutation results belong to the original SHA above; they were not re-run for an
import-only change. The all-targets check was repeated at the repair SHA: exit 0.

The five new `data_reads` tests were repeated on repair SHA
`9d75aa3b919f7ea80e0e148c4dec6470b645b577`: exit 0, 5 passed, 0 failed/ignored.
The second scoped index run exited 1 because each partial-receiver target needed
an explicit exact-target rename note. Metadata-only follow-up
`4b868ea036e4316113eed7c31162d21b79e2e370` adds those nine notes. The Rust targets,
row statuses and bucket decisions did not change.

Final gate checkpoint is `4b868ea036e4316113eed7c31162d21b79e2e370`:

- `git diff --exit-code 9d75aa3b919f7ea80e0e148c4dec6470b645b577 HEAD --
  <crate>/src <crate>/tests`: exit 0, preserving the repeated compile/test inputs.
- Crate `cargo fmt --check`: exit 0.
- `python3.12 tests/parity_scope.py`: exit 0, real invention/status gates pass
  **99 rows and 52 public Rust items**. No global parity claim.
- `parity:extract` repeated: exit 0, updated 0 manifests and no added/orphaned rows.
- `parity:extract:check` repeated: exit 0, would update 0 manifests.

The earlier compile/test/control exits are attached to their actual input SHAs
above, with the import-only/body-byte and metadata-only/source-byte comparisons.

## Integration merge-down

Fetched and merged integration
`c1b0ce61f2247f5604b26ed5a00b142be75d90ef` down before push. The sole conflict was
`parity/exempt.toml`; resolution preserves every incoming `(rs, reason)` record
plus exactly the six BingX additions relative to the merge base. Python 3.12
validated TOML shape, unique keys and the exact expected record set. No helper
exemption or other lane's contribution was dropped.

`git diff --exit-code 4b868ea036e4316113eed7c31162d21b79e2e370 -- <crate>/src
<crate>/tests` exited 0 against the resolved merge working tree. The own-source
and test gate inputs survived byte-for-byte; no repeat matrix was needed. The
post-commit HEAD comparison is the final pre-push verification. Gate exits remain
claims about the recorded checkpoints, not a workspace build on the merged head.

# Position, configuration and cancellation slice — 2026-10-14

Gate input SHA: `35462a1ddec33c294e079690f1818fd95591f861`.
Fresh branch `feat/rust-venue-bingx-positions` starts at integration
`41505c870788695cb1e3201843360997f3d22794`, with no unreviewed feature ancestor.
Python 3.12 `tomllib` compared all BingX rows by `(ts, kind)` to that exact parent:
**14 ported / 24 divergent / 61 pending → 14 ported / 32 divergent / 53 pending**.
This slice moves **0 pending→ported, 8 pending→divergent**, all with distinct
concrete method targets and distinct notes. Class, constructor, API factory,
remaining trading methods and websocket rows stay pending.

## This slice's own bucket decisions

Every new row is **CHOSEN**, not FORCED. The exact boundaries below persist even
when the admitted cases match. The shared string restriction is
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`; typed fields use
`#closed-typed-output-boundary`, and snapshot identity uses
`#owned-snapshot-vs-live-alias`. These decisions do not depend on ApexOmni or
on the preceding BingX slice's precedent.

| Source row | Concrete `BingxDataReads` method | CHOSEN difference and own evidence |
| --- | --- | --- |
| `fetchOpenPositions` | `fetch_open_positions` | Shared Position.side refuses inherited callable hits. Actual source with positionSide=constructor returns a function side and id `BTCUSDT:function Object() { [native code] }`; native reports refusal and returns cached positions. All source row evaluations precede refusal: a later liquidationPrice coercion error wins in both runtimes. Returned positions/info and cached fallback are owned Vec/Value snapshots rather than source aliases, and identifiers/info use UTF-8. Four live cases compare filtering, first-hyphen replacement, raw notional arithmetic, signed contracts, side fallback/override, truthy isolated flag, margin fallbacks and strict code handling. |
| `fetchPositions` | `fetch_positions` | Source returns/reuses mutable position arrays and info objects; native returns owned position snapshots and UTF-8 identifiers. The concrete open-position side refusal and typed metadata numeric refusal also affect this result, rather than admitting arbitrary erased values. Four live cases distinguish absent symbols and empty lists, preserve selected-position order and duplicate missing-symbol requests, execute actual metadata bodies and check each zero-valued placeholder field/ID directly. |
| `fetchPositionMetadata` | `fetch_position_metadata` | Metadata is a typed numeric/enum record using UTF-8; source erased leverage values may be strings. Pending siblings after a rejected pair use shared BaseExchange task ownership/disposal, with a visible late rejection diagnostic; source Promise.all has no host registration at that edge. Actual-source and native delayed-sibling witnesses prove the parent returns empty before leverage completes, then leverage still updates Store. Four live cases cover concurrent requests within serial symbols, duplicate symbols, null-leverage skips, hedged long/short ordering, one-way rows and disposal. A separate in-flight test proves the post-await guard after both requests start. |
| `changePositionMode` | `change_position_mode` | Publishes typed owned Position values/UTF-8 info, rather than source runtime records and aliases; failures are structured ExchangeError, excluding arbitrary primitive throws. Four live cases check true/false string POST bodies, strict numeric code, forced mode refetch and actual position fetch/Store.update. A real failing Store subscriber proves the write remains applied, the original name/cause/response preference survives, and IDs append inner-to-outer. |
| `changeMarginType` | `change_margin_type` | Symbol is Option<String>, so arbitrary erased symbol values/unpaired UTF-16 and primitive throws cannot cross the configured edge. Refresh uses the concrete typed position boundary. Four live cases check preflight before HTTP, quote-aware ID, ISOLATED/CROSSED body, strict response handling, forced mode refresh, symbol-limited fetch and shared add_or_update_positions effects. |
| `setLeverage` | `set_leverage` | The shared untransformed RestBody cannot carry NaN. Actual source forwards NaN at this configured HTTP seam; native visibly refuses before POST/refresh instead of projecting it to null. Supported numbers use RetainedJsonInput::Float. Closed PositionSide and UTF-8/structured-error arguments exclude source erased values. Four live cases distinguish market-first validation, hedge-side requirement, BOTH/SHORT body, lower/upper clamps, response and actual position refresh effects. |
| `cancelOrders` | `cancel_orders` | Inputs are closed shared Order records with UTF-8 IDs/symbols; source erased records/getters and primitive throws are outside that edge. Four live cases check integer-key-first group order through shared js_array_index, initial venue-ID resolution, serial requests, per-ID failures with continuation, strict numeric 0/80016 acceptance and canceled status/update counters. A failing real Store subscriber proves the completed first group's status persists and later groups stop while the original error boundaries survive. |
| `cancelSymbolOrders` | `cancel_symbol_orders` | Shared Order records, UTF-8 symbols and structured errors exclude arbitrary source records/throws. Four live cases prove the intentionally unchecked DELETE response code, post-await filtering of current Store orders, other-symbol preservation, canceled-status effects and report-before-rethrow with original transport subscriber IDs. |

`PositionMetadata` is the sole new named anonymous-return-shape exemption; it
contains no independent venue body. The configured HTTP capability's operation
was directly cut over from `ReadTransport::get` to `ReadTransport::send` to admit
GET/POST/DELETE. Its single exemption was renamed accordingly; no stale get
exemption or fallback implementation remains. Existing read tests were adapted
for the source await boundary: configured responses yield before decoding, so
both metadata requests start before an immediately ready leverage result writes
Store. This is the necessary call capability, not a pending venue-method stub.

## Witnesses and measured gates

`positions_match_actual_typescript` executes the eight maintained source bodies
and their already-landed mode/leverage/margin/symbol helpers. The oracle also
executes the actual TypeScript DefaultStore, clone, createPositionId, get-key,
protocol tables/enums and actual lodash/p-iteration modules. The clock capability
returns 1234; no real-time or network behavior is inferred. Source input snapshots
are isolated before execution so Store mutation cannot rewrite the input oracle.
Native tests use the production shared DefaultStore and BaseExchange.

The fixed floor is **32 cases, four per row**, with independent method/variant
ordering and input fingerprint `40d8d59c58f15663`; expected values come from live
source execution. Native position/metadata fields, numeric bits, enum variants,
request configs, Store fields and update counters are observed before any wire
encoding. Additional boundaries measure the inherited callable, later source
error precedence, NaN request, and a delayed sibling after rejection. Five other
native tests cover those boundaries, disposal and subscriber failures. Each test
executable caches one source corpus. Recorded source runtime: Node `v26.7.0`,
`arm64`, `darwin`.

At the gate SHA above, explicit `/opt/homebrew/opt/rustup/bin/cargo`, SDK manifest,
`-p safe-cex-venue-bingx`, `--jobs 1`, `--locked`:

- `cargo check --all-targets`: exit 0.
- Explicit targets `positions` and `data_reads`: exit 0; **11 passed,
  0 failed, 0 ignored** (6 new-family tests and 5 existing read tests).
- Real Yarn `parity:extract`: exit 0; updated 0 manifests, no added/orphaned rows.
- Real Yarn `parity:extract:check`: exit 0; would update 0 manifests.

Not run: full BingX suite, workspace check/test, full monorepo suites, host or wasm
clippy, wasm build/test, global parity audit, CI, live HTTP/credential checks,
API-factory/signing/double-parse checks, or lifecycle/websocket gates. No shared
type or shared trait changed; ReadTransport is venue-local. No production
credentials or network requests were used. This is body-by-body account REST
coverage, not full venue assembly or arbitrary JavaScript runtime parity.

- Crate `cargo fmt --check`: exit 0.
- Scoped real parity index `python3.12 tests/parity_scope.py`: exit 0,
  **99 rows / 61 public Rust items**, invention and status gates passed.
- `python3.12 tests/positions_controls.py`: exit 0. It stopped after these four
  production controls, one per new family. Each compiled and ran one native test,
  failed its named assertion with exit 101, then restoration passed with exit 0.
  No survivor. Full logs are diagnostic only under ignored target, not the evidence.

| Family | Production operation changed | Named native assertion |
| --- | --- | --- |
| position reads | notional subtracts upnl instead of adding it | `BINGX-POS native fetchOpenPositions variant 0` |
| metadata | short row uses buyLeverage instead of sellLeverage | `BINGX-POS native fetchPositions variant 0` |
| configuration | clamp maximum uses market minimum | `BINGX-POS native request order/config setLeverage variant 0` |
| cancellation | numeric 80016 is no longer accepted | `BINGX-POS native Store effects cancelOrders variant 0` |

A separate Python check inspected all four broken logs for the completed Cargo
compile, one running native test and the named left/right assertion failure.
`git diff --exit-code 35462a1ddec33c294e079690f1818fd95591f861 -- <crate>/src
<crate>/tests` exited 0 after all controls: production and test bytes restored.

## Merge-down preservation

Fetched and merged integration
`9d7e0523cd16502a66901c3bf264e222bc5081ba` down before push. The only conflict was
`parity/exempt.toml`. Relative to the merge base, this slice removes the retired
own `ReadTransport::get` exemption, adds `ReadTransport::send`, and adds
`PositionMetadata`. Resolution preserves every other incoming `(rs, reason)`
record exactly. Python 3.12 validated the complete expected set, unique keys and
TOML record shape; no other lane's contribution was removed.

The resolved-merge worktree comparison to gate SHA
`35462a1ddec33c294e079690f1818fd95591f861` over this crate's `src` and `tests`
exited 0. Own gate inputs survived byte-for-byte, so the recorded gate exits
stand without repeating controls. Post-commit HEAD preservation and tomllib
census are the final checks before push. No merged-head workspace build is claimed.

# BingX public websocket core slice

Fresh branch `feat/rust-venue-bingx-public-stream`, cut from requested integration
`b5d642aea03ced39a3cc27bb95767b8fd242c3b7`. Code/oracle/control checkpoint:
`e4d8e39aa10cfed71190b5ac5fc1d12e27135986`.

Independent Python 3.12 tomllib census: **8 rows moved: 1 pending → ported,
7 pending → divergent**. BingX totals: **15 ported / 39 divergent / 45 pending**,
99 rows. The source websocket class, listenOHLCV and listenOrderBook stay pending.
Every moved row has its own distinct note and concrete Rust declaration.

## This slice's own bucket decisions

These are this slice's decisions, independent of the preceding BingX/ApexOmni
slices. String restrictions refer to
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`; closed model fields to
`#closed-typed-output-boundary`; output identity to `#owned-snapshot-vs-live-alias`.
All native bodies below are in `src/public_stream.rs`.

| Source row | Concrete Rust declaration | Bucket and evidence |
| --- | --- | --- |
| parsePositiveFiniteNumber | parse_positive_finite_number | **ported**. The Number intrinsic is an explicit fallible dependency, invoked once. The new body implements the finite-and-positive predicate, returns the original number bits, and propagates arbitrary error identity. Sixteen independently pinned numeric inputs include NaN, both infinities, both zeros, subnormal/extreme positives; the actual source helper supplies expected outputs. A source throwing-coercion spy and native Rc error identity witness both preserve the original error. This does not claim a new arbitrary-JS ToNumber implementation. |
| getDiagnosticSymbols | BingxPublicStream::get_diagnostic_symbols | **CHOSEN**: owned UTF-8 keys/output and Vec identity replace UTF-16 keys and Set identity. Local prefix/capture and insertion deduplication match four actual-source cases, including duplicate BTC, empty capture, nonmatching topics, newline and astral symbols. The shared boundary is a deliberate representation choice. |
| formatMarketContractsDiagnostic | BingxPublicStream::format_market_contracts_diagnostic | **CHOSEN**: configured JSON excludes accessors and prototype identity; string spread operates on UTF-8 scalars. The measured astral-data source result contains two escaped surrogate fields, whereas the native result contains one crab scalar. Four admitted cases match exact diagnostic strings, including replacement of existing contracts, filtering, primitive spread and null. |
| connectAndSubscribe | BingxPublicStream::connect_and_subscribe | **CHOSEN**: native listeners use weak Rc ownership and structured errors, whereas JS bound functions retain their receiver. The owner-release test gets visible errors from both registered native callbacks; the retained-owner test executes onOpen and onMessage through the registered listeners. Four source/native cases preserve URL selection, disposed guard and open/message/close registration order. |
| onOpen | BingxPublicStream::on_open | **CHOSEN**: host IDs use UTF-8 String and failures use ExchangeError, excluding arbitrary UTF-16/JS throw identities. The body locally sends the contracts subscription, replays topics and pings in order. Four actual-source cases cover disposed/empty/quoted-ID and nonpositive-count paths. The source-ordered trace includes the exact JSON payload strings and raw interpolated heartbeat string. |
| ping | BingxPublicStream::ping | **CHOSEN**: native reports the caught structured error through its host; source silently discards it. The failing-Ping case measures identical attempted sends plus the additional native send-cause report. Other cases preserve pingAt-before-send, two literal frames, raw unescaped interpolation and disposal. Reporting instead of silence is an intentional behavior choice. |
| subscribe | BingxPublicStream::subscribe | **CHOSEN**: UTF-8 topic/ID inputs exclude JS lone-surrogate keys. Local Object.keys snapshot replay retains numeric-key order and subscribes zero/negative counts even when disposed. Four source/native cases cover each of those paths and empty topics. No count filtering or extra disposal guard was added. |
| onMessage | BingxPublicStream::on_message | **CHOSEN**: valid JSON lone surrogates visibly fail native ingress; source delivers them to callbacks. Native contracts are arrays; source string iterable witness succeeds with no ticker writes. Typed shared ticker maps refuse inherited callable targets and reacquire the index per contract rather than capturing the JS map across subscriber replacement. Fourteen admitted source/native cases retain heartbeat priority, diagnostics-before-heartbeat, Blob await/disposal, callback append/replacement, own-empty/prototype callback handling, malformed frames and duplicate ticker bits. No arbitrary iterable/map identity parity is claimed. |

The new stream carrier is partial state for these bodies. Its injection-only
`new`, callback representations and host capability declarations have individual
exemptions. They do not complete the pending source class or listener bodies.
No pending venue body is delegated to a host: the imported operations are
BaseWebSocket/socket, clock/nanoid, Blob, diagnostics and stats dependencies.

## Witnesses and measured gates

`tests/public_stream_oracle.cjs` extracts the actual seven source field bodies,
the numeric helper and the already-landed source deriveTickerBidAsk body. It
executes the actual source DefaultStore/clone, get-key, safe-math/isNotNumber,
jsonParse, protocol constants and readBlob with real pako gzip/Blob input. Native
uses the production shared DefaultStore/BaseExchange, ModelPatch numeric writes
and imported socket/Blob capabilities. Clock capabilities are 1234 monotonic and
1700000000000 epoch milliseconds; IDs are fixed fixture inputs. Native Blob
capability supplies the independently specified ASCII decoded input; native gzip
decompression is not implemented or claimed.

The independent floor is **38 stream cases**: four per method plus ten additional
onMessage paths, with pinned family/variant ordering and sorted-input FNV
fingerprint **217dc1f0d03a4bb0**. The guard has **16 separately pinned native numeric
inputs**, one-operation/error-identity assertions, and live source expectations.
Ticker output fields are measured from native f64 values as bits before wire
encoding. Source/native callback logs preserve live captured-array append and
replacement behavior. The native subscriber-failure test preserves name, message,
response, inner IDs 71/83 plus the outer subscriber ID, and the first applied
write before rejection. Boundary tests directly compare the priced UTF-16 spread,
lone-surrogate and iterable differences. Runtime: Node `v26.7.0`, arm64, darwin.

At the code checkpoint above, explicit `/opt/homebrew/opt/rustup/bin/cargo`, SDK
manifest, `-p safe-cex-venue-bingx`, `--jobs 1`, `--locked`:

- `cargo check --all-targets`: exit 0.
- Explicit test targets `public_stream` and `data_reads`: exit 0; **11 passed,
  0 failed, 0 ignored** (6 new stream tests and 5 existing read tests).
- Crate `cargo fmt --check`: exit 0.
- Real Yarn `parity:extract`: exit 0; updated 0 manifests, 0 added/orphaned rows.
- Real Yarn `parity:extract:check`: exit 0; would update 0 manifests.
- Scoped real parity index `python3.12 tests/parity_scope.py`: exit 0;
  **99 rows / 90 public Rust items**, invention and status gates passed.

Not run: full BingX suite, workspace check/test, full monorepo suites, host/wasm
clippy, wasm build/test, global Rust parity audit, CI, live HTTP/WebSocket or
credential checks, API-factory/signing checks, full lifecycle/listener assembly,
or native Blob decompression. No shared type or shared trait changed. No
production credential or network request was used by these gates.

- `python3.12 tests/public_stream_controls.py`: exit 0. Exactly four controls,
  one per new family, each compiled and ran one native test, failed its named
  assertion with exit **101**, then restored and passed with exit **0**. The
  runner verifies completed compilation, one running test and the named failure;
  no survivor. Diagnostic logs remain ignored under `sdk-rs/target/`.

| Family | Changed production operation | Named failed assertion |
| --- | --- | --- |
| numeric guard | positive predicate accepts zero | BINGX-GUARD positive finite 3 |
| diagnostics | contracts filter selects nonmembers | BINGX-PUBLIC native formatMarketContractsDiagnostic variant 0 |
| connection/subscription/heartbeat | initial subscription names market.wrong | BINGX-PUBLIC operation order onOpen variant 0 |
| frames/ticker writes | mark reads indexPrice instead of fairPrice | BINGX-PUBLIC native ticker bits 0 |

Post-control `git diff --exit-code
 e4d8e39aa10cfed71190b5ac5fc1d12e27135986 -- <crate>/src <crate>/tests` exited 0:
production, oracle, tests and control bytes restored to the checkpoint.

## Integration merge-down preservation

Fetched and merged integration `77746a5b498ce3d3532424345f072425181f33ca` down
before push; merge completed without conflicts. Scoped `git diff --exit-code
 e4d8e39aa10cfed71190b5ac5fc1d12e27135986 HEAD -- <crate>/src <crate>/tests`
exited 0: all own production/oracle/control/test inputs survived byte-for-byte.
A further scoped diff found no changes to the shared num/store/exchange/types/
transport crates or source BingX/store/utils/json-parse/math dependencies since
the gate checkpoint. No repeat mutation run or merged-head workspace build is
claimed.

Independent post-merge tomllib comparison against the requested parent confirms
**1 pending → ported and 7 pending → divergent**, eight distinct indexed Rust
targets, eight distinct notes, and CHOSEN in every new divergent row's own note.
BingX remains **15 ported / 39 divergent / 45 pending**, 99 rows. A separate set
comparison confirms all 776 exemption `(rs, reason)` records equal the incoming
set plus this slice's 21 own helper additions; nothing from another lane was
lost. Final HEAD source/test preservation and census checks precede push.

# BingX private fill/account/refresh slice

Fresh branch `feat/rust-venue-bingx-private-events`, based on requested integration
`3a957718fe445a833bb10a464e16e7cc6c585276`. Code/oracle checkpoint:
`c27f92f0a1bc85d7906e3b016215dfc8e7e9303f`.

Control-runner checkpoint: `5adbbcb1c6541752e181999cd31fff1a07676f3a`.
Only the mapping control changes between these checkpoints; production, native
Rust tests and source oracle are identical. The initial mapping mutation removed
the only subtract use and failed compilation, so the runner rejected it before
counting any assertion proof. The corrected mutation negates the filled operand
while retaining subtract; the measured controls below use this correction.

Independent Python 3.12 tomllib census: **8 rows moved: 1 pending → ported,
7 pending → divergent**. BingX totals: **16 ported / 46 divergent / 37 pending**,
99 rows. Seven private-websocket bodies and the mapOrder dependency are mapped;
the private class/constructor, listen-key lifecycle and onMessage remain pending.
Every moved row has a distinct own note and an indexed Rust declaration.

## This slice's own bucket decisions

These decisions are local to this slice, independent of previous slice precedent.
String restrictions use `sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
closed model fields use `#closed-typed-output-boundary`, and retained identity uses
`#owned-snapshot-vs-live-alias`. All targets below are in `src/private_events.rs`.

| Source row | Concrete Rust declaration | Bucket and evidence |
| --- | --- | --- |
| BingxExchange.mapOrder | map_order | **CHOSEN**: typed Order enums/options and owned UTF-8/JSON info replace arbitrary JS enum/prototype values and row aliases. Actual source timeInForce=`constructor` returns a callable; native rejects it visibly. Actual source unknown stop trigger has an own undefined field; native has None/absence. Four admitted cases compare Number conversions, aliases, strict flags, stop-price fallback, existing/post-only/default timeInForce, reduce-only, trigger tables, clock and decoded remaining bits. |
| ping | private_ping | **ported**: the entire disposed/clock-assignment/literal-send body uses imported operations with generic error type E. Clock, assignment and send can preserve arbitrary failure identity. Four source cases compare ordering, disposal and send rejection. Source throwing-clock/send spies preserve an opaque object; native preserves Rc identity, assigns infinity unchanged, and makes no disposed operation. No dynamic string or closed output field is introduced by the body. Full receiver/class construction is not claimed. |
| handleOrderEvents | BingxPrivateEvents::handle_order_events | **CHOSEN**: typed shared Order, UTF-8 keys and configured JSON payloads exclude callable field/prototype identities; absent native optional fill fields replace own JS undefined fields. Local cumulative/direct fill logic, terminal-only emission, cache limit/deletion, mapped nonmarket order writes and strict wire-ID status matching are implemented. Four cases plus three full event sequences compare effects. Partial duplicate updates are retained; terminal cache deletion permits terminal replay to emit again, exactly as source. Numeric wire IDs do not update a stored string ID through coercion. |
| handleAccountEvents | BingxPrivateEvents::handle_account_events | **CHOSEN**: shared typed Position/side/mode fields and owned info replace inherited callable sides and live row identity. Native also visibly reports the mode error source catch discards, then uses the same fallback. Four cases compare hedged/one-way/inferred/zero sides, invalid quantity, normalized prices/PnL/liquidation, existing ID/leverage/mode, aliases, margin selection and scheduling. Original Store subscriber errors are propagated through ExchangeError conversion. |
| normalizeSymbol | BingxPrivateEvents::normalize_symbol | **CHOSEN**: configured JSON input coercion and UTF-8 output exclude lone-surrogate JS strings. Actual source lone-surrogate input returns UTF-16 units D800/X, which native String cannot represent. Four admitted cases compare truthy-or-empty String conversion and removal of every hyphen, including zero, noncallable coercion error and astral scalar. |
| extractAccountPositions | BingxPrivateEvents::extract_account_positions | **CHOSEN**: native returns owned JSON row snapshots; source filter returns live row references. Actual source input mutation changes an extracted row's entry price; native snapshot retains the original value. Four cases compare P/B/p candidate priority, position-like field presence, nonarrays/nonobjects, empty symbol, and first nonempty filtered candidate. |
| schedulePositionRefresh | BingxPrivateEvents::schedule_position_refresh | **CHOSEN**: native timeout callbacks have weak ownership and structured host errors; JS closure retains its receiver. Native owner-release callback visibly fails. Async native execution starts when the imported executor polls the future; arbitrary executor scheduling identity is excluded. Four cases and an exact two-event coalescing/650 ms delay/750 ms boundary witness compare pending-before-guard, immediate throttle branch, scheduled flag, disposal and callback execution. |
| executePositionRefresh | BingxPrivateEvents::execute_position_refresh | **CHOSEN**: native reports caught original structured failures; source silently catches them. Actual source Store subscriber failure prevents loaded=true and resolves without throwing; native also stops before loaded=true and records name/message/response, inner subscriber IDs 71/83 plus the outer ID. Shared typed Store flat-position info/extra are absent options, while source writes own undefined fields. The test asserts those two exact shape differences before comparison. Four cases use real landed position REST bodies and shared Store, including REST failure fallback and disposal after await. |

The carrier and injection-only `new` remain helpers with explicit exemptions.
Its host interface exposes clock/disposal, timers/tasks and fill/error reporting;
no pending venue method is delegated. mapOrder is a real new body here. Mode and
position reads invoke the already-landed BingxDataReads production methods.
No shared type or shared trait changed.

## Source/native witnesses and gates

`tests/private_events_oracle.cjs` extracts all eight actual source bodies, plus
already-landed fetchPositions/fetchOpenPositions/getOrFetchPositionMode. It runs
the actual source DefaultStore/clone, createPositionId, get-key, safe-math/
isNotNumber, protocol tables and enums. REST transport and clock/timer/emitter
intrinsics are controlled inputs. Native executes shared DefaultStore and
BaseExchange with the real landed REST bodies. Source emitAndLogError is an
imported call capability; the full base error-reporting pipeline is not compared.
Native base-runtime exception reports and this slice's own exception reports are
captured separately so no dependency report is falsely attributed to a new body.

The independent floor is **32 cases, four per row**, with family/variant census
and sorted-input FNV fingerprint **bbd1b521471ed9f5**. Expected outputs come from
live source execution; working input snapshots are isolated before mutation.
Three additional fill sequences cover partial/repeated/terminal/cache-delete
replay, NaN cumulative/direct fill, and numeric ID strict matching. Additional
witnesses cover timer coalescing/execution, callback owner release, opaque clock/
send errors, source live row alias, callable timeInForce, own undefined triggers,
source lone surrogate, and refresh subscriber rejection. Native Order/Position
numeric fields are compared as f64 bits before serialization. Runtime: Node
`v26.7.0`, arm64, darwin; virtual clock/performance 1234 and fixture Date.now
100/750/1000. No real timer or network behavior is inferred.

At the checkpoint above, explicit `/opt/homebrew/opt/rustup/bin/cargo`, SDK
manifest, `-p safe-cex-venue-bingx`, `--jobs 1`, `--locked`:

- `cargo check --all-targets`: exit 0.
- Explicit targets `private_events` and `positions`: exit 0; **13 passed,
  0 failed, 0 ignored** (7 new tests and 6 existing position tests).
- Crate `cargo fmt --check`: exit 0.
- Real Yarn `parity:extract`: exit 0; updated 0 manifests, 0 added/orphaned rows.
- Real Yarn `parity:extract:check`: exit 0; would update 0 manifests.
- Scoped real parity index `python3.12 tests/parity_scope.py`: exit 0;
  **99 rows / 107 public Rust items**, invention and status gates passed.

Not run: full BingX suite, workspace check/test, full monorepo suites, host/wasm
clippy, wasm build/test, global Rust parity audit, CI, live HTTP/WebSocket or
credential checks, API-factory/signing checks, private message/BigInt/Blob ingress,
listen-key/lifecycle/full venue assembly, native Blob decompression or real timer
scheduling. These gates used no production credentials or live network requests.

- `python3.12 tests/private_events_controls.py`: exit 0. Exactly five corrected
  controls, one per new family, each compiled and ran one native test, failed its
  named assertion with exit **101**, then restored and passed with exit **0**.
  The runner checks completed compilation, one running test and the named failure.
  No survivor. Logs remain diagnostic-only under ignored `sdk-rs/target/`.

| Family | Production operation changed | Named failed assertion |
| --- | --- | --- |
| order mapping | remaining subtracts negated filled | BINGX-PRIVATE native mapOrder variant 1 |
| fills/order effects | positive increment must exceed three | BINGX-PRIVATE fill payloads handleOrderEvents variant 2 |
| account extraction/transformation | notional subtracts upnl | BINGX-PRIVATE native positions handleAccountEvents variant 0 |
| position-refresh scheduling/execution | loaded.positions becomes false | BINGX-PRIVATE loaded flag schedulePositionRefresh variant 1 |
| heartbeat | sends Pong instead of Ping | BINGX-PRIVATE calls ping variant 0 |

Merge-down preservation uses the corrected control-runner checkpoint
`5adbbcb1c6541752e181999cd31fff1a07676f3a`. Its production/native Rust test/source
oracle bytes are identical to the code checkpoint; a scoped git diff exited 0.
The complete source/test tree comparison after all controls also exited 0.

## Integration merge-down preservation

Fetched integration `671cdadf56e6ec1c1955795f0976a8cfd0d28d09` and merged it down
before push. The only conflict was `parity/exempt.toml`; resolution parsed all
three stages and preserved the complete incoming `(rs, reason)` set plus this
slice's nine helper additions. Python validated all **839 records**, unique keys
and exact TOML record shape; no incoming contribution was removed.

Resolved-tree `git diff --exit-code
 5adbbcb1c6541752e181999cd31fff1a07676f3a -- <crate>/src <crate>/tests` exited 0:
production, native tests, source oracle and corrected controls survived unchanged.
A further scoped diff found no changes to shared num/store/exchange/types/transport
or source BingX/store/utils/types/math dependencies since the code checkpoint.
The measured gates stand; no merged-head workspace build or repeat control run
is claimed. Final HEAD preservation is checked again before push.

Post-merge independent tomllib totals remain **16 ported / 46 divergent /
37 pending**, 99 rows. Relative to the requested parent: **1 pending → ported,
7 pending → divergent**, eight distinct indexed targets and eight distinct own
notes, each divergent note containing its own CHOSEN justification.

# BingX listen-key lifecycle slice

Fresh branch `feat/rust-venue-bingx-listen-key`, based on requested integration
`2a8f733506ff1bb167076694e18a3032e96088a1`. Code/oracle/control checkpoint:
`10c3c96a0d30eda08ddabb17ec812a4a4fed88ce`.

Independent Python 3.12 tomllib census: **9 rows moved: 1 pending → ported,
8 pending → divergent**. BingX totals: **17 ported / 54 divergent / 28 pending**,
99 rows. The private class, constructor and onMessage remain pending. Every moved
row has a distinct own note and a concrete indexed Rust target.

## This slice's own bucket decisions

These decisions are independent of previous slice precedent. String restrictions
refer to `sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`, and owned
value identity to `#owned-snapshot-vs-live-alias`. All targets below are in
`src/listen_key.rs`.

| Source row | Concrete Rust declaration | Bucket and evidence |
| --- | --- | --- |
| clearListenKeyRefreshTimer | clear_refresh_timer | **ported**: imported truthiness, clear and unset operations accept arbitrary token type T and arbitrary error type E. The body clears only a truthy token, then unsets the slot. A failed clear preserves slot/token and the original error. Four source cases cover undefined, zero and nonzero timer values. Actual source opaque-object/throw spies compare with native Rc token/error identity and clear-before-assignment. The generic target does not impose u64/ExchangeError restrictions. |
| connectAndSubscribe | BingxListenKey::connect_and_subscribe | **CHOSEN**: socket handles/listener descriptors and structured errors replace JS socket/bound-function/throw identity, and key URL strings use UTF-8. Local clear/fetch/connecting/catch/finally/disposal/interpolation/registration operations are implemented. Four source cases plus overlapping fetch, post-await disposal, retry and create failures compare effects and flags. URL keys remain raw interpolation, including query delimiter characters; no encoding correction was introduced. |
| onOpen | BingxListenKey::on_open | **CHOSEN**: the clock/socket error binding is structured and the called refresh scheduler uses native timer/weak-owner bindings. The body locally schedules refresh before invoking already-ported private_ping; the second disposed check remains in ping. Four source cases compare timer, pingAt and literal Ping order. |
| fetchListenKey | BingxListenKey::fetch_listen_key | **CHOSEN**: configured numeric-preserving JSON/UTF-8 values and owned key snapshots exclude arbitrary JS values and live response aliases. The actual-source returned object key aliases stored state; native mutation of the returned object leaves the stored key unchanged. Four cases plus numeric/object keys compare POST-without-body, truthy top-level/nested fallback, missing-response JSON.stringify error and state assignment. Transport errors preserve original cause metadata. |
| updateListenKey | BingxListenKey::update_listen_key | **CHOSEN**: UTF-8/owned key and structured message/response values exclude arbitrary getters/throw/coercion identity. The body sends PUT with undefined body and key params, renews on success, warns and chooses rebuild versus 30-second retry. Four cases plus three expired-key paths and eight regex strings compare strict numeric 404, ASCII ignore-case, UTF-16 optional unit and LF/CR/U+2028/U+2029 behavior. Native UTF-16-unit matching preserves BMP-versus-astral behavior for admitted strings. |
| rebuildWithFreshListenKey | BingxListenKey::rebuild_with_fresh_listen_key | **CHOSEN**: optional socket ID/listener descriptors and structured errors replace JS object/bound-function/throw identity. Local disposed/rebuilding guards, timer/key clearing, message-close-open removal, ws unset before close, connect/catch/recovery/finally are implemented. Four cases and create/close failures preserve source pre-try failure leaving rebuilding=true, versus caught failure restoring false. |
| scheduleListenKeyRefresh | BingxListenKey::schedule_listen_key_refresh | **CHOSEN**: u64/weak-owner timers and structured task errors replace JS opaque handles, retained receiver identity and unhandled-rejection identity; f64/Option delay excludes arbitrary JS coercion. Local clear-before-schedule, default 1800000 ms and explicit 17.5 ms match source. A fired callback executes the newly implemented update body, and a released-owner callback returns a visible error. |
| scheduleListenKeyRecovery | BingxListenKey::schedule_listen_key_recovery | **CHOSEN**: u64/weak-owner timer and structured task failure bindings replace opaque handle/strong receiver/unhandled-rejection identity. The body locally clears before scheduling exactly 30000 ms, even if disposed. A fired callback executes the newly implemented rebuild body; source/native traces and flags match. No real timer backend behavior is inferred. |
| dispose | BingxListenKey::dispose | **CHOSEN**: native timer/optional-key/structured base-dispose bindings exclude JS handle/property/throw identity. The body clears timer, unsets key and invokes imported BaseWebSocket disposal in that order. Four cases and a base-dispose failure witness preserve earlier writes and connecting/rebuilding flags. Full class teardown is not claimed. |

The carrier, injection-only new, u64 clear binding, listener descriptor and host
operations have individual exemptions. Listener descriptors only register/remove
identities; neither the pending constructor nor onMessage body is delegated to a
host or claimed here. HTTP uses configured ReadTransport and the protocol table;
no shared type or shared trait changed.

## Witnesses and measured gates

`tests/listen_key_oracle.cjs` extracts the actual nine source bodies, the already
ported ping body and actual refresh/retry constants. It runs actual source
protocol enums/tables with controlled HTTP, socket, BaseWebSocket and clock/timer
intrinsics. Native bodies use the real configured request and numeric-ingress
interfaces; the socket/timer operations remain imported capabilities.

The independent floor is **36 cases, four per row**, with fixed family/variant
census and sorted-input FNV fingerprint **632c05493eab5a18**. Expected outputs come
from live source execution; working input snapshots are isolated. **11 additional
cases** cover create/post/close/retry/base-dispose failures, post-await disposal,
strict expired-key recovery and nonstring keys. **Eight regex cases** cover BMP,
astral, case, optional separator and four line terminators. Further witnesses
execute both timer callbacks, serialize overlapping connections, measure opaque
token/error identity, reject released native owner, and price the owned-key alias
boundary. Native key values, timer/socket numeric bits, flags and HTTP/host effects
are asserted before wire encoding. No expected output is reused as native input.
Runtime: Node `v26.7.0`, arm64, darwin; performance capability returns 1234.

At the checkpoint above, explicit `/opt/homebrew/opt/rustup/bin/cargo`, SDK
manifest, `-p safe-cex-venue-bingx`, `--jobs 1`, `--locked`:

- `cargo check --all-targets`: exit 0.
- Explicit targets `listen_key` and `private_events`: exit 0; **15 passed,
  0 failed, 0 ignored** (8 new tests and 7 existing private-event tests).
- Crate `cargo fmt --check`: exit 0.
- Real Yarn `parity:extract`: exit 0; updated 0 manifests, 0 added/orphaned rows.
- Real Yarn `parity:extract:check`: exit 0; would update 0 manifests.
- Scoped real parity index `python3.12 tests/parity_scope.py`: exit 0;
  **99 rows / 137 public Rust items**, invention and status gates passed.

Not run: full BingX suite, workspace check/test, full monorepo suites, host/wasm
clippy, wasm build/test, global Rust parity audit, CI, live HTTP/WebSocket or
credential checks, API-factory/signing/double-parse checks, private message/
BigInt/Blob ingress, full constructor/class/base teardown assembly or real timer
scheduling. These gates used no production credentials or live network requests.

- `python3.12 tests/listen_key_controls.py`: exit 0. Exactly four controls, one
  per new family. Each compiled and ran one native test, failed its named
  assertion with exit **101**, then restored and passed with exit **0**. The
  runner checks completed compilation, one running test and the named failure;
  no survivor. Logs remain diagnostic-only under ignored `sdk-rs/target/`.

| Family | Production operation changed | Named failed assertion |
| --- | --- | --- |
| key acquisition/connection | top-level truthy key is rejected | BINGX-KEY native fetchListenKey variant 0 |
| renewal/rebuilding | finally retains rebuilding=true | BINGX-KEY native state updateListenKey variant 1 |
| open/refresh/recovery timing | default refresh is three minutes | BINGX-KEY operation order onOpen variant 0 |
| clearing/disposal | timer truthiness guard is inverted | BINGX-KEY operation order connectAndSubscribe variant 0 |

Post-control `git diff --exit-code
 10c3c96a0d30eda08ddabb17ec812a4a4fed88ce -- <crate>/src <crate>/tests` exited 0:
all production, native/source tests and control bytes restored to the checkpoint.

## Integration merge-down preservation

Fetched integration `d5c0e7d58a7f03c8163680335e7b632ebb43ec86` and merged it down
before push. The only conflict was `parity/exempt.toml`. Resolution parsed all
three stages and preserves the complete incoming `(rs, reason)` set plus this
slice's 21 helper additions. Python validated all **893 records**, unique keys
and exact record shape; no incoming contribution was removed.

Resolved-tree `git diff --exit-code
 10c3c96a0d30eda08ddabb17ec812a4a4fed88ce -- <crate>/src <crate>/tests` exited 0:
production, source/native tests and controls survived byte-for-byte. A further
scoped diff found no changes to shared num/exchange/types/transport or source
BingX/types dependencies since the checkpoint. The recorded gates stand; no
merged-head workspace build or repeat control run is claimed. HEAD preservation
is checked again after the merge commit and before push.

Post-merge independent tomllib totals remain **17 ported / 54 divergent /
28 pending**, 99 rows. Relative to the requested parent: **1 pending → ported,
8 pending → divergent**, nine distinct indexed targets and nine distinct own
notes, with each new divergent note containing its own CHOSEN justification.

## PR 7647 conflict repair against 1674d852f7

Merged `origin/feat/safe-cex-rust` at
`1674d852f7687b8d427cd4b77c3aee067688979f` into this branch, producing merge SHA
`ac8f29c91f7b9ffcd72d107e457282ce3c2ce376`. The only conflict was
`parity/exempt.toml`; three-stage TOML resolution preserves all incoming records
plus the original 21 listen-key helper additions. Python validated all **920
records**, exact `(rs, reason)` union, unique keys and record shape.

At that merged SHA, the explicitly requested scoped gates were rerun using
`/opt/homebrew/opt/rustup/bin/cargo`, SDK manifest, this crate, `--jobs 1 --locked`:

- `cargo check --all-targets`: exit 0.
- `cargo test --test listen_key --test private_events`: exit 0; **15 passed,
  0 failed, 0 ignored** (8 lifecycle tests, 7 private-event tests).
- Scoped source/test comparison to original gate SHA
  `10c3c96a0d30eda08ddabb17ec812a4a4fed88ce`: exit 0; all own inputs preserved.

The final follow-up commit adds this receipt only. No full suite, workspace,
clippy/wasm, CI, live network or repeat mutation run was performed for this repair.
The original nine moved rows and their per-row decisions remain intact.

## Order-placement slice: eight new rows

Fresh branch `feat/rust-venue-bingx-order-placement` starts at integration
`1674d852f7687b8d427cd4b77c3aee067688979f`. It does not depend on the unmerged
listen-key feature branch. The independent tomllib delta selects eight previously
pending placement rows: zero pending to ported, eight pending to divergent.
Each has a distinct real `placement.rs` target and its own CHOSEN note.

The native bodies own side selection, normal/trailing payload construction,
splitting/remainder arithmetic, attachments, parallel preparation, serial POSTs,
cache updates, five-item legacy chunks and error handling. Dependencies are the
landed shared Store/Base caller-ID guards/mode reads/mapper and configured HTTP.
`PlacementHost` supplies the imported UUID operation, not a placement body.
The complete exchange class and constructor remain pending.

### Bucket justification in this slice

| Row | CHOSEN boundary and independent witness |
| --- | --- |
| createClientOrderId | Native UTF-8/scalar slicing cannot return a source lone UTF-16 surrogate. Actual source ends at code unit D83D; native retains the complete astral scalar. Shared boundary: MANIFEST-SPEC.md#shared-utf8-string-boundary. |
| getOrderPositionSide | Typed side/type enums and bool refuse source truthy string reduceOnly. Actual source flips the side; native deserialization visibly rejects the string. |
| formatCreateOrder | Actual source produces quantity NaN; native JSON payload construction returns a visible refusal. Attached source protection owns undefined reduceOnly, while native omits that field; both slots are asserted before the narrowly priced comparison. |
| formatCreateTrailingStopLossOrder | Actual source inverse side lookup for constructor returns a callable; closed native OrderSide rejects that options input. Market/ticker/position and finite-price checks remain real native logic. |
| placeOrder | The real wrapper propagates native formatter NaN refusal before POST; actual source reaches POST with quantity NaN. Optional JSON IDs preserve numeric IDs and missing slots, rather than pretending the source string[] annotation describes all results. |
| placeOrders | Typed option clones are owned snapshots of source live objects. NaN POST refusal is directly witnessed. All formatters start and a pending sibling survives early rejection; native additionally reports later unawaited sibling errors through the runtime. Shared boundary: MANIFEST-SPEC.md#owned-snapshot-vs-live-alias. |
| placeOrderBatch | Actual source caches an inherited callable timeInForce. Native typed mapper refuses it, reports the failure and returns a final placement error. Typed JSON responses/errors cannot carry arbitrary JavaScript prototypes/getters. |
| placeOrderBatchOld | Actual source executes a response-owned callable forEach and returns its ID. Native JSON cannot carry that callback and visibly refuses a non-null callback property. Plain JSON-array includes failure is preserved, not corrected. |

These are deliberate typed representation choices, so CHOSEN applies to all
eight. Finite nominal behavior does not erase the measured differences to claim
ported parity. See each row's own manifest note for its target and justification;
no bucket rests on an earlier ApexOmni or BingX slice.

### Oracle and limits

The AST loader executes all eight actual TypeScript members, the actual Base
caller-ID guards, actual Store and numeric/helpers. The fixed floor stops after
variant 3 of each method: **32 cases**, plus **twelve named extras**, with native
outputs, binary64 bits, request order/body, cache fields and visible error labels
asserted. Additional tests cover surrogate slicing, typed bool/enum refusals,
three actual-source NaN formatter/wrapper paths, callable legacy iteration,
inherited mapper TIF and an early-rejected deferred formatting sibling.

Configured HTTP/UUID capabilities are controlled inputs. No live BingX account,
exchange network, production UUID generator, full exchange constructor, browser,
workspace build/test, clippy, WASM, double-parse or CI run is claimed. Exhaustive
JavaScript object, Unicode casing, Promise/microtask and legacy response behavior
is not claimed. Generated logs/corpus stay ignored under sdk-rs/target.

The four additional JSON-error extras preserve null response code-access errors
and null/numeric legacy message includes errors, including the original report
labels and continued partial effects. The configured source HTTP fixture keeps
null responses intact instead of replacing them with a fallback response.

Initial controls at code checkpoint
`51a807e98bcd86d3ef24e90abe5a74755a77edf3` completed UUID, side, normal formatter,
trailing formatter, parallel wrapper and submission failures/restorations
(101 then 0). The legacy mutation also compiled and failed the earlier named
`BINGX-PLACE native output placeOrderBatchOld 2` assertion; the runner incorrectly
expected a later request assertion and exited 1. Production was restored in
finally. This is an expectation correction, not a surviving mutation. The runner
now names the observed native output assertion; affected submission/legacy
controls are rerun against the JSON-error additions below.

### Measured gates at the merged code checkpoint

Merged integration `407c879ef469c4ad7f702980834d11e80aacedaa`, including the repaired
listen-key branch, producing **`2d41381cf5f9f9f771ea209e346ce26a41210b6d`**.
The exemption conflict preserved the exact incoming set plus four native carrier
additions: **926 records**, unique keys and exact record shape validated. The
append conflict in this evidence file retains both complete slice receipts.
Own placement source/tests compared to `89971eb4ec` with scoped git diff: exit 0.
The whole crate changed because listen-key files/module were added, so the
following scoped gates were rerun at this merged code SHA:

| Gate | Measured outcome |
| --- | --- |
| cargo check, this crate, --all-targets --jobs 1 --locked | exit 0 |
| cargo test, explicit placement/listen_key/private_events targets, --jobs 1 --locked | exit 0; 20 passed, 0 failed/ignored (5 + 8 + 7) |
| parity:extract writer | exit 0; Updated 0 manifests, 0 added/orphaned |
| parity:extract:check | exit 0; Would update 0 manifests, 0 added/orphaned |
| real scoped parity index | exit 0; 99 rows, 149 public native items |
| affected submission mutation | completed compile, running 1 test, exit 101 at BINGX-PLACE decoded Store placeOrder; restored exit 0 |
| corrected legacy chunk mutation | completed compile, running 1 test, exit 101 at BINGX-PLACE native output placeOrderBatchOld 2; restored exit 0 |
| complete source/test preservation after controls | git diff --exit-code against this merged checkpoint: exit 0 |

The six earlier family controls are measured at `51a807e98b`; the two affected
submission/legacy controls above are measured at the merged code checkpoint.
No claim that all controls were repeated at the merged SHA is made. There are
no surviving mutations. The unchanged normal/side/trailing/wrapper/UUID bodies
retain their earlier family controls; JSON-error additions pass the new extras.

Independent tomllib comparison against the actual integration parent confirms
**0 pending -> ported, 8 pending -> divergent**, with eight distinct Rust targets
and eight distinct notes. Combined BingX total: **17 ported / 62 divergent /
20 pending**, 99 rows. The earlier nine listen-key rows are integration work and
are not counted again as this slice's delta.

The explicit nonclaims above still apply. This receipt is a documentation-only
follow-up; source/test bytes are checked again after the final integration
merge-down and before push.

### Final integration preservation before push

Final merge-down parent: `95b56a79bc646156e1812b17428e0fb6295413a5`.
Merge SHA: `370bb2a8a27a11ca9adc10be2210c8f567b4b2c3`.
The exemption EOF conflict preserves the exact incoming set plus the same four
native carriers; **958 records**, unique keys and exact shape validated.

`git diff --exit-code 2d41381cf5f9f9f771ea209e346ce26a41210b6d HEAD --
<crate>/src <crate>/tests` exited **0**. A separate scoped diff of shared
exchange/num/types/store/transport, SDK Cargo manifest/lock and source
BingX/Base/client-ID/Store/utils/types/math dependencies also exited **0**.
All measured code gates stand; no new compile/test/control run is claimed at
this final preservation merge. A documentation-only receipt follows it.

Tomllib comparison to the final integration parent again confirms **0 pending
-> ported, 8 pending -> divergent**, eight distinct targets/notes, and **17
ported / 62 divergent / 20 pending**. The repaired listen-key SHA
`57e73f25efdd9498603562033abda7bf01a2c000` is an ancestor of integration; its nine
rows are included there and are not this slice's new work.

## REST history, subscriptions, dispatch and polling: ten new rows

Fresh branch `feat/rust-venue-bingx-read-streams` begins at requested integration
`0e116b60e90e37e08f3995645493fc20176c0b86`. Both earlier listen-key and placement
slices are merged ancestors. Independent tomllib selects exactly ten previously
pending rows: **3 pending -> ported, 7 pending -> divergent**, ten distinct Rust
targets and ten distinct notes. BingX totals become **20 ported / 69 divergent /
10 pending**, 99 rows. None of the earlier rows is counted again.

### This slice's own bucket decisions

| Row | Decision and witness |
| --- | --- |
| exchange.listenOHLCV | PORTED generic forwarding: opaque opts/callback/result/error identities survive unchanged; actual source returned object/undefined, alias mutation and throw are measured. |
| exchange.listenOrderBook | PORTED generic forwarding: the symbol/callback pair reaches the bound listener once, with the original opaque result or error. Native Rc identity is checked independently. |
| exchange.dispose | PORTED ordered generic calls: Base, public, private; first error stops the sequence and preserves its opaque identity. Actual source and native test all three failure boundaries. |
| fetchOrders | CHOSEN closed typed output and owned snapshots: MANIFEST-SPEC.md#closed-typed-output-boundary and #owned-snapshot-vs-live-alias. Actual callable TIF is paired with native refusal. Truthy code fallback, both order containers and mark-dependent stop-to-TP conversion are real local body logic. |
| fetchOHLCV | CHOSEN existing typed OHLCVOptions/Timeframe/query carrier. Source infinite from becomes a NaN startTime and reaches GET; native explicitly reports refusal and returns the empty error fallback. Limit defaults/cap, from precedence, date arguments, candle fields and ordering run against actual source. |
| fetchHistoricalFills | CHOSEN shared Execution.order_id is Option<String>, with no venue-only raw-ID model. Source numeric ID 77 remains numeric; native reports the refusal while retaining the completed prefix. Three containers, Date/parseInt timing, fallback fields, code checks, guards and logs run locally. |
| public.listenOHLCV | CHOSEN shared Timeframe is retained rather than widened across consumers. Source 1M subscribes to 1month; native enum admission rejects it. Deferred/duplicate/early cleanup and the source's uncancelled pending candle retry are preserved. |
| public.listenOrderBook | CHOSEN MANIFEST-SPEC.md#shared-utf8-string-boundary, with paired topic code-unit/admission witness. Native shares handler/topic state with the landed public receiver and keeps the live book Rc. Reference counts, depth fields/totals, early timer cancellation, cleanup and partial mutation are asserted. |
| private.onMessage | CHOSEN MANIFEST-SPEC.md#shared-utf8-string-boundary, with surrogate ping paired against visible refusal. Own body routes text/Blob frames and diagnostics, calls the shared big-int parser, preserves Ping/Pong precedence, dispatches expired/account/order/TRADE events and records stats. Native production adapter calls the landed event/key bodies. |
| tick | CHOSEN native scheduled callbacks hold a Weak receiver to avoid a self-owned polling cycle. Actual source retained callback executes after external owner release; native returns a visible released-owner error. Counts, disposal guards, loaded flags, full position replacement, catch/report and next scheduling run locally. |

The three generic bodies do not introduce a typed value conversion. Their
opaque native result/error tests measure identity rather than comparing a
cloned representation. The seven differences stay divergent; they are not
weakened to call the whole slice ported. Every bucket is justified here and in
its own manifest note, without relying on an earlier slice's decisions.

### Measured floor and scope

The AST loader executes ten actual members across the three source files. It
loads actual Store, mapOrder/read dependencies, number helpers, Dayjs and the
big-int parser. Imported timer/loop/socket/disposal/Blob operations are controlled
capabilities. The floor stops after variant 3 of each member: **40 cases**, plus
**12 fixed extras**, not an exhaustive input universe.

Six native tests assert decoded values and binary64 bits before call traces:
full floor/extras, opaque forwarding/disposal identity, history representation
boundaries, stream representation boundaries, polling owner release and retained
book partial mutation. Book bids assigned before a failed asks parse remain
observable through the old callback reference; totals wait until both sides
complete. The real shared receiver handler arrays are replaced, preserving the
source's per-dispatch array identity and cleanup behavior.

Thirty-four explicit native helper exemptions describe receiver/callback and
imported-operation bindings. The full exchange/socket classes, constructors,
API factory, startup and amendments remain pending. No pending venue method is
implemented by a host mock or claimed as a complete class.

No full workspace build/test, complete crate test suite, clippy, WASM, browser,
live exchange, production API/signing/double-parse, CI, exhaustive Promise or
calendar/local-time/Unicode/object matrix is claimed. Date fixtures use UTC;
calendar subtraction is an imported Dayjs capability. Generated corpus and
mutation logs remain ignored under sdk-rs/target.

### Gate exits bound to the merged SHA

Initial code checkpoint: `8f6b39e739895b9a856af88b491209b7b343c3ee`.
Forwarding call traces were tightened to compare the actual argument Rc at
`6237de1d0af622947d346be06c9b2f64f61565bb`; the independent opaque identity test
was already present. No production or manifest change accompanied that test fix.

Integration `9fdb2a05ffe14554db00996c29ab70cb5a12db1c` merged down into code SHA
**`fcbd4394b5982cd3dfb90cbfebf43b956543a640`**. The only conflict was exemption
EOF: exact incoming union plus all 34 own helpers, **1053 records**, unique keys
and record shape verified. Source/tests compared to 6237de1d0a unchanged (exit 0).
The integration lockfile changed, so check/tests were rerun at the merged SHA.

| Gate at merged code checkpoint | Exit and measured scope |
| --- | --- |
| cargo check, BingX --all-targets --jobs 1 --locked | 0 |
| cargo test, explicit read_streams/public_stream/private_events/listen_key targets, --jobs 1 --locked | 0; **27 passed**, 0 failed/ignored (6 + 6 + 7 + 8) |
| parity:extract:check | 0; Would update 0 manifests, 0 added/orphaned |
| real scoped parity index, after all controls restored | 0; **99 rows, 193 public native items** |
| entire crate source/tests diff after controls | 0 against merged code checkpoint; no temporary mutation remains |

The manifest writer ran before the initial checkpoint with the same manifest
inputs: exit 0, Updated 0 manifests, 0 added/orphaned. It is not claimed to have
rerun at the merged SHA. All following controls ran against the merged code
checkpoint, using the one fixed native floor/extras test. Each completed compile,
ran one test, failed its named assertion with **101**, and restored with **0**:

| New family | Deliberate production break | Named assertion |
| --- | --- | --- |
| order history | converted stop becomes LIMIT | BINGX-RS decoded native output fetchOrders 0 |
| candles/registered candle parser | timestamp divisor becomes 100 | BINGX-RS decoded native output fetchOHLCV 0 |
| fills | commission sign inverted | BINGX-RS decoded native output fetchHistoricalFills 0 |
| generic forwarding | discard returned value and panic; caller independently asserts completed invocation | BINGX-RS native call completed exchange.listenOHLCV |
| book parser | decoded asks assigned to bids | BINGX-RS decoded native output public.listenOrderBook 0 |
| private routing | Ping replies Ping | BINGX-RS calls private.onMessage 0 |
| polling | order-read count offset becomes 1 | BINGX-RS decoded Store tick |
| disposal | public disposal precedes Base | BINGX-RS calls dispose 0 |

No survivor or compiler-only control is claimed. Opaque identity and live-book
assertions additionally run in their separate native tests; wire/call traces
follow decoded output/Store checks.

Post-merge tomllib comparison to the actual integration parent again measures
**3 pending -> ported, 7 pending -> divergent**; ten unique targets and notes.
The totals and explicit nonclaims above stand. This receipt changes documentation
only; final integration preservation is checked before push.

### Final integration preservation

Final merge-down parent: `157f83240a85280c25df395ba6a51225813fe9bf`.
Merge SHA: `664ab832b6032a38f98fa7ba863b694f743b34ed`.
Exemption EOF resolution preserves exact incoming union plus the same 34 own
helpers: **1057 records**, unique keys and exact shape validated.

Entire crate source/tests diff against measured merged code checkpoint
`fcbd4394b5982cd3dfb90cbfebf43b956543a640` exited **0**. Shared source dependency
inspection found only a Cargo.lock change adding already-resolved base64/crypto
dependencies to Blofin. Independent lock parsing validated exact equality of the
full resolved **481-package BingX closure** and **19-package parity-check
closure**; Blofin is the only changed lock record. No relevant library source,
SDK Cargo manifest or source BingX/helper input changed.

Measured gates therefore stand without another compile/test/control run at this
preservation merge. The final documentation-only receipt is followed by another
source/test diff before push. No survivor or temporary source mutation remains.

Independent tomllib comparison to this integration parent measures **3 pending
-> ported, 7 pending -> divergent**, ten distinct targets/notes, and **20 ported
/ 69 divergent / 10 pending**, 99 rows. All previous explicit nonclaims apply.

## Venue completion: final ten pending rows

Base: `0e014c0aa040874ed83ad9f13d9ae617c57a6435`, fresh branch
`feat/rust-venue-bingx-completion`. This slice implements the API factory and
interceptor, exchange constructor/startup/amendments, and complete public/private
websocket carriers. The carriers expose the landed read/history/placement and
stream/subscription/key/event/dispatcher bodies; bound private message callbacks
invoke `NativePrivateDispatchOperations`, not a supplied venue surrogate.

Independent manifest expectation: **10 pending rows** at the base. This slice
moves **0 pending -> ported, 10 pending -> divergent**, yielding **20 ported /
79 divergent / 0 pending**, 99 rows. Every new row has a distinct production
Rust target and its own note. Earlier notes that described now-mapped consumers
as pending are corrected without changing those earlier rows' statuses.

### Own bucket justifications

These justifications belong to this slice; none relies on an earlier slice's
classification. All ten are **CHOSEN**, for an observable native boundary:

| New row | Native target | Difference and reason |
| --- | --- | --- |
| createAPI | api.rs::create_api | Owned request/options do not mutate caller params/body/header aliases; source timestamp and POST header edits remain observable to its caller. See MANIFEST-SPEC.md#owned-snapshot-vs-live-alias. Native non-object/NaN signing admission also visibly refuses unsupported RestBody values; source quantity='notnumber' produces NaN after HMAC. Native prototype-mutating signing keys visibly refuse, while source reduction assigns the accumulator prototype. The owned typed contract avoids an unchecked mutable transport object. UTF-8 and pinned ICU root locale remain explicit. |
| BingxExchange | exchange.rs::BingxExchange | Complete composition owns typed components and a configuration snapshot. Source full class observes caller option edits after construction; native Base keeps its snapshot. See MANIFEST-SPEC.md#owned-snapshot-vs-live-alias. Component typed/weak-owner boundaries remain class differences. |
| constructor | exchange.rs::BingxExchange::new | Base and both API clients receive owned options rather than retaining the original mutable caller record. Independent source post-construction key mutation/native unchanged key witnesses the choice; see MANIFEST-SPEC.md#owned-snapshot-vs-live-alias. Construction locally passes the actual API closure through imported secondary/rate-limit operations, then constructs the independent unlimited client and both carriers. |
| start | exchange.rs::BingxExchange::start | Native unawaited private-task failures and failed retained siblings report their causes; source leaves private Promise rejection unhandled and Promise.all consumes subsequent sibling rejection. Visible failure reporting is deliberate. A real private socket creation rejection demonstrates the private-task difference. Arbitrary event-loop/Promise identities are not claimed. |
| updateOrder | amendment.rs::BingxAmendment::update_order | Closed native Order/UpdateOrderOpts ingress rejects invalid runtime enums/empty updates before the body. Source side='constructor' reaches immutable-identity preflight rejection instead. Allowing unchecked side/status/type/TIF would weaken the shared order contract. |
| replaceTriggerOrderPrice | amendment.rs::BingxAmendment::replace_trigger_order_price | Native typed side admission rejects the source's arbitrary side before trigger preflight; source runs the body and rejects reconciliation. The shared typed Order remains closed. Cancel-before-place, raw trigger-kind override and trigger/close/reduce choices execute locally. |
| cancelReplaceLimitOrderPrice | amendment.rs::BingxAmendment::cancel_replace_limit_order_price | Invalid caller side reaches source replacement identity rejection, but native admission rejects first. The typed Order contract is retained. Refresh/reconciliation, payload, strict result confirmation, old cancellation and new insertion are implemented locally. |
| BingxPrivateWebsocket | websocket.rs::BingxPrivateWebsocket | Weak bound owners prevent an Rc host/callback cycle, while source bound closures retain their receiver. Saved source close still runs after the outer venue is released; native saved close visibly refuses a released key owner. The complete carrier composes actual key/events/dispatcher bodies and shares the native read parent. |
| private constructor | websocket.rs::BingxPrivateWebsocket::new | The clear-timer-before-Base-close wrapper forwards the original event but uses Weak key ownership. The retained source close versus named native released-owner refusal independently justifies the constructor choice, avoiding a callback ownership cycle. |
| BingxPublicWebsocket | websocket.rs::BingxPublicWebsocket | Ordered native own-entry topic state cannot contain JS inherited callable properties; source empty topics.constructor is a function, native has no such entry. This keeps topic counters explicit. The complete carrier shares one actual stream between all public bodies and subscriptions and binds inherited close. |

### Native/source observation scope

`completion_oracle.cjs` compiles the maintained complete exchange/public/private
classes with TypeScript; new class and constructor bodies are not substituted.
Imported Base, HTTP/socket/timer/UUID and external-data operations use controlled
capabilities. The API factory is compiled in full separately; actual qs,
retry-read classification and bigint parser are imported, with a recorded HMAC
intrinsic returning an opaque signature. Cryptographic implementation, HTTP
rate timing and Base runtime internals are outside this slice's claim.

Independent floor: **40 cases**, four per new row (36 class/body cases plus four
API cases), followed by **11** fixed amendment extras. Extra boundary witnesses
cover caller option/request aliases, public prototype state, weak owner release,
invalid order-side admission, NaN POST conversion, prototype-mutating signing keys and private-task rejection.
Source fixture inputs are cloned before invocation so cancellation cannot alter
the captured native input. Native order numeric bits, enum/ID/retained fields,
balance bits/info and constructor Rc identities/default clocks are checked
before serialization/call comparisons. Source ordered HTTP/socket/timer/log
traces and real Store loaded/order/balance effects are compared.

Native imported capabilities have **29 exact helper exemptions**. No wildcard
exemption, placeholder row or new shared core type/trait is introduced.

Gate receipts and full code checkpoint SHA are recorded below after measurement.
Nonclaims: no full crate/workspace suite, workspace build, host/WASM clippy,
WASM compilation, live exchange/credentials/network validation or CI run.

### Terminal null reconciliation correction

Final inspection found that a nullish chain's last term can remain `null`;
filtering every alias incorrectly turned it into undefined. Native alias lookup
now preserves terminal null and filters it only when the source has an explicit
nullish default. New actual-source cases cover zero-size amendment with terminal
quantity null, quantity reconciliation refusal and a replacement ID whose final
alias is null. These supported JSON states match the source; they are not used
to inflate divergence counts. Native loaded flags also have direct field checks
before wire observations.

### Close registry ownership

The complete carriers additionally use Weak inherited-close bindings so a host
that owns its callback registry cannot retain itself through those callbacks.
The native ownership test saves public close, releases the venue and host,
asserts that the host Weak reference cannot upgrade, then observes the named
bindings-released error. A saved actual-source public close still invokes Base
close after the outer venue is released. This is another explicit CHOSEN
lifetime difference for the public class and private class/constructor, justified
by avoiding native ownership cycles; it does not weaken any row to ported.

### Measured gate receipts

Initial merge-down parent: `c394ef75eec20388a2dbb80d9d69c0bf0fa69a4c`.
The exemption conflict resolution preserved the exact incoming union plus all
29 new native helpers: **1119** unique records with exact record shape.
Initial merged code checkpoint: `e1c524b89bda0346a5cbd215bbcee9304197adbe`.

| Checkpoint | Measured gate | Result |
| --- | --- | --- |
| e1c524b89bda0346a5cbd215bbcee9304197adbe | Scoped cargo check --all-targets --jobs 1 --locked | 0 |
| e1c524b89bda0346a5cbd215bbcee9304197adbe | Explicit completion/listen_key/placement/positions/private_events/public_stream/read_streams targets | 0; 45 passed (7 + 8 + 5 + 6 + 7 + 6 + 6) |
| 82d6b32aba39a0e650b661e2db18125a622223de | Corrected completion target, including terminal-null and prototype witnesses | 0; 8 passed |
| 82d6b32aba39a0e650b661e2db18125a622223de | All eight compiled production controls | Every control 101 at a named assertion, then restoration 0 |
| 03b827c025caa13f3315bd094486abb511ee5695 | Final close-binding cargo check --all-targets --jobs 1 --locked | 0 |
| 03b827c025caa13f3315bd094486abb511ee5695 | Final completion target including host release | 0; 9 passed |
| 9853a85c2b996a5f205fbbca9cb3ab1751cf290b | Changed public/private constructor controls repeated | Both compiled 101 at a named assertion, then restoration 0 |
| 9853a85c2b996a5f205fbbca9cb3ab1751cf290b | Extractor writer and check | Both 0; Updated/Would update 0 manifests, 0 added/orphaned |
| 9853a85c2b996a5f205fbbca9cb3ab1751cf290b | Real scoped parity index after restoration | 0; 99 rows, 232 public native items |

The final runner-selector commit changes only Python control selection; Rust
compiler inputs and the runtime TypeScript oracle compare unchanged to the
03b827c025 checkpoint. The six existing dependency test targets (**38 tests**)
and their venue bodies compare unchanged to e1c524b89b. Final completion replaces
the earlier seven-test floor with **9**, giving **47 distinct selected tests**
across these explicitly recorded checkpoints, not a full-crate suite claim.
The API/amendment/startup source files compare unchanged to 82d6b32aba; only the
changed websocket constructor families needed another compiled control run.

| Family | Production break | Named assertion |
| --- | --- | --- |
| API | timestamp incremented | BINGX-C API prepared 0 |
| exchange constructor/class | secondary limit becomes 4 RPS | BINGX-C calls exchangeClass 0 |
| startup | public-only balance-loaded flag false | BINGX-C loaded start |
| quantity amendment | wrong returned order ID | BINGX-C output updateOrder 0 |
| trigger replacement | cancel empty order slice | BINGX-C direct native order status |
| cancel-replace | old order remains Open | BINGX-C direct native order status |
| public class | insert ghost topic during construction | BINGX-C output exchangeClass 0 |
| private constructor/class | close schedules refresh instead of clearing it | BINGX-C output exchangeClass 0 |

Every mutation finished compilation and ran the named test. No survivor or
compiler-only control is counted. Original production bytes were restored after
each control. Whole crate source/tests compare to 9853a85c2b with exit **0**,
including the working tree; no deliberate mutation remains. Logs/corpus remain
ignored under sdk-rs/target and are not committed.

Tomllib confirms **0 pending -> ported, 10 -> divergent**, ten distinct native
targets and ten distinct own notes, and **20 / 79 / 0**, 99 BingX rows. Final
integration merge-down/preservation is recorded next. The explicit nonclaims
above still apply.

### Final integration preservation

Integration parent: `3cc94529835dde67ba3e6af513a233baa307c3f0`.
Final preservation merge: `6e3cdb0d778974555123532d9305b30e4e7b08fc`.
The merge was clean. Entire BingX src/tests compare to measured code checkpoint
`9853a85c2b996a5f205fbbca9cb3ab1751cf290b` with exit **0**.

Relevant shared library/source/oracle/compiler-input inspection found only a
lockfile change adding a dependency to Weex. Independently parsed complete
resolved dependency closures compare exactly equal: **481 packages** for BingX,
**19 packages** for parity-check. No relevant library source, SDK Cargo manifest,
extractor/indexer source or BingX/helper source changed. The measured scoped
gates therefore stand; no repeat compile/test run is claimed for this merge.

Final-parent tomllib delta again measures **0 pending -> ported, 10 -> divergent**;
**20 ported / 79 divergent / 0 pending**, 99 rows, ten distinct new targets and
own notes. This preservation receipt changes documentation only; the complete
source/test diff is checked once more before push. Nonclaims remain unchanged.

## PR 7717 integration conflict repair

Integration `0ffe13d5ef70c19c137639eafaad6bc118fa6c9a` merged down into
`feat/rust-venue-bingx-completion` at measured merged code SHA
**`3a9bde4431d92e2424dc3e33650d19e165babba6`**.
The only conflict was `sdk-rs/parity/exempt.toml`. Resolution preserves the exact
incoming union plus all 29 BingX helper entries: **1153** unique records, exact
record shape verified. No venue implementation or test was changed.

Gates rerun at that merged SHA, sequentially with --jobs 1 --locked:

| Gate | Measured result |
| --- | --- |
| cargo check -p safe-cex-venue-bingx --all-targets | 0 |
| Explicit completion/listen_key/placement/positions/private_events/public_stream/read_streams tests | 0; **47 passed**, 0 failed/ignored (9 + 8 + 5 + 6 + 7 + 6 + 6) |
| Real scoped parity index | 0; **99 rows, 232 public Rust items** |
| Entire BingX src/tests compared to previously pushed 6c12b79564fec0546abf3c0c1c31b062b0504718 | 0; unchanged |

Tomllib comparison to this integration parent preserves the original slice's
**0 pending -> ported, 10 -> divergent**, with ten distinct own notes and targets;
BingX is **20 ported / 79 divergent / 0 pending** on the branch. This repair moves
no additional rows. The original compiled mutation receipts remain valid because
all production/test bytes are unchanged; mutations were not repeated here.
This receipt changes documentation only. No workspace/full suite, clippy, WASM,
live exchange, CI or extractor run is claimed for this conflict repair.

Final pre-push fetch also merged integration
`eb5339fff8f489456695778fe2af9701f8c18b69` at
`c27715f55ff7c5e58ae81eff2aff2ad10dbd4e75`; it adds only Hyperliquid work.
The complete BingX src/tests/Cargo manifest/parity rows, shared source dependencies,
SDK Cargo manifest/lock, extractor/indexer and source BingX/helper inputs compare
unchanged to the measured merged SHA 3a9bde4431 with exit **0**. The 47-test/check/
index receipts therefore stand. This final preservation receipt is documentation
only and is followed by a complete source/test diff before push.


## Production Exchange binding and registry, 2026-10-16

The crate-root BingxExchange assembles the existing venue bodies with shared
js_runtime HTTP, intrinsics, socket cores and Store; no transport or value-model
replacement is used. Required 16 Exchange hooks have individually named E2Es.
The source inherits base.ts:799-801,989-991,1138-1140 rejections for max leverage,
schedule cancel and public trades; the tests require the exact Error and no I/O.

- Full crate suite with the actual TypeScript compiler: 92 passed (65 existing,
  27 assembled host tests). Host all-targets and wasm strict Clippy pass.
- binding-controls.py: all 16 deliberate wrong Exchange bodies compiled and
  failed their exact named E2E; restoration passed. Separate controls for both
  public listener overrides compiled, failed and restored (18 bound_ tests).
- Public/private gzip and zlib Ping frames each produce Pong through the real
  shared binary bridge and shared read_blob inflater. Both listeners receive
  actual protocol frames and unsubscribe. The book callback snapshot cost is
  explicit at its manifest row and is asserted after cleanup.
- HTTP proofs cover primary/secondary keys, caller-ID rejection before requests,
  bigint wire IDs, one re-signed signature refusal (HTTP200 and400), fresh read
  signatures, no ambiguous-write replay, and preserved HTTP status/request/hints.
- Native registry/constructor tests: 15 pass including BingX startup, a server-side
  HMAC check, placement/cancellation and existing Binance signature resend. Removing
  the BingX table entry compiles and fails construction; restoring it passes.
- Global parity invention/status gate: 0 findings after current master integration, 7459 rows /17061 Rust items.
  Read-only extractor: no BingX or registry drift; shared base/safeCexAxiosCreate
  metadata remains routed to its owners (0 added/0 orphaned symbols).
- registry:check passes. layers:check is 86/88: the two existing Lighter missing-
  classification assertions expect an old incoming edge; BingX exact membership
  and all actual graph validations pass. The overseer has the failure details.

Shared raw-response preservation and binary delivery are independent prerequisites
#149 and #160. Existing per-row venue divergences remain; this evidence does not
claim whole-venue equivalence. Hosted signing is visibly unavailable in this host.

Placement uses fractional inputs amount1.2345/price100.127 against steps .001/.01;
the actual maintained TypeScript adjust helper returns1.235/100.13. Separate
binding-precision-controls.py mutations swap each production precision operand;
both compile and fail their named wire quantity/price assertion, then restore.
This strengthens the binding witness: integral inputs alone would mask a swap.


## Shared HTTP factory follow-up, October2026

The assembled host now delegates HTTP construction and transport-adapter selection
to the landed shared factory. Current source safeCexAxiosCreate overwrites the
venue timeout with20_000ms and merges Axios's Accept default. BingX's source
installTransportAdapter call omits the variant argument, selecting Default.
The prior native route instead retained extra.recvWindow, omitted Accept and
requested BigInt. The three newly authored E2Es compiled against that production
body and failed at those exact values:1234 versus20_000, missing Accept versus
`application/json, text/plain, */*`, and BigInt versus Default.
The custom adapter also observes the signature-rejection resend, proving both
attempts use the selected transport while the scripted fallback remains unused.
Raw-text bigint parsing and per-attempt retry signing remain on their existing
bodies. No shared crate, registry or manifest edits are needed for this repair.

The binding-factory-controls.py controller independently reintroduces each wrong
behavior in the new bridge, rejects compiler errors as evidence, and restores
production bytes in finally. Final gate counts and compiled-control receipts are
reported in the follow-up PR before handoff.

Measured on base master d2110dd1ec: full BingX suite95 (65 source witnesses and30
assembled E2Es), three compiled factory regressions with three passing restores,
host all-targets and wasm library strict Clippy, workspace fmt and global parity0
(7459rows/17107public Rust items). Logs: /tmp/bingx-factory-{before,after,controls,
full,clippy,wasm,fmt,parity}.txt. Native registry proof is reported separately once
its loopback run completes.

Native native_host_bingx also passes1 over real loopback HTTP/WS, checking startup,
wire HMAC, bigint caller/order identifiers, placement and cancellation. Receipt:
/tmp/bingx-factory-native.txt. No live credentials or exchange requests were used.


## Native hosted signer (ddi2il-23)

Source BingX createAPI skips local preparation when options.remoteSigner exists;
the shared source remoteSigningAdapter owns the browser round trip. Rust imports
that function as the actual shared HTTP host capability, not a serialized options
object. ExchangeHost.signers attaches the same signer to BaseExchange before Rc
assembly, so a secretless hosted account takes the private startup path. The API
host capability suppresses local HMAC even when a stray local secret remains.
The native registry passes its account signer/account name through the shared
rest_http_host_signed path. PayloadSigner remains a separate unavailable capability.

The new native_host_bingx test compiled/fails on the previous native-host refusal.
With the capability wired, both old local signing and the new hosted proof pass.
Hosted proof runs two complete local HTTP/WS lifecycles, with empty and deliberately
unused local secrets, checks the actual network order against browser-side HMAC
using source canonical key sorting, and observes caller ID/bigint cancellation.
It then uses a fresh non-polling fixture to prove signer refusal sends no HTTP.
The flat ASCII fixture codec is deliberately bounded; the existing actual-source
API witnesses cover the general source serializer. Test development excluded its
initial invalid serialized-function metadata attempt and corrected the canonical
server comparison after Map::remove reordered a key; those are not negative proofs.

hosted-signing-controls.py independently removes native HTTP signer forwarding,
BaseExchange signer attachment and local-signing bypass. All three compile/fail
the named native E2E and restore to pass. Relevant source/test bytes are restored
before any other Cargo invocation. Only the two reviewed constructor body hashes
change in the shared dispatch inventory; its other37 BingX rows and every other
lane's rows remain unchanged. Final scoped gate results are reported in the PR.

After merging master5e76c883d1 at64a5aef5e1: full venue95 and native registry2
pass; strict host all-targets Clippy for venue/registry, wasm library Clippy and
workspace fmt pass. Global invention/status parity0 (7459rows/17118public items)
after mapping both new trait capability members to source createAPI/constructor.
Cancellation inventory7 including its temporary exporter and four override tests
including live TS inventory pass. Full dispatch census stops only at two stale
Hyperliquid base_exchange_class body hashes from master; all39 BingX rows match.
Control receipts precede this master integration; positive suites reran afterward.
Logs /tmp/bingx-hosted-{before,after,controls,full,native-final,clippy,wasm,fmt,
parity,audits}.txt. No live credentials, venue requests or deployment used.
