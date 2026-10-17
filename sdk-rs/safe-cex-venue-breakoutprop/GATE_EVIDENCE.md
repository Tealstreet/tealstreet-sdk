# Reviewed inherited refusals: complete native owner cohort

After merging the shared inherited-refusal mechanism, this branch installs only
five independently reviewed Breakout specs in
`accepted-set/inherited-constant-outputs/safe-cex-venue-breakoutprop.json`.
Their Rust source, maintained TS base/venue, fixture, compiler APIs, native adapters
and cases match the approved v2 bytes. No source or fixture rollback was used.
The lifecycle owner retains all nine actual compiler entrypoints; get_account and
schedule_cancel are not invented as indexed APIs.

Fresh default native rustdoc/check, actual compiler artifacts, alias claims,
macro origins and local-scope guards cover **60 owners + 7 aliases**. All
**67/67 native receipts pass** (47 production runtime results, 20 explicit
classifications, zero failed owners). This includes the complete nine-API
lifecycle owner and the ten actual refusal observations: exact Error/Not
implemented, default metadata, zero HTTP and unchanged observed native state.
The ProductionHost Clone construction obligation remains separately classified.

The cohort-aware shared policy tests pass 2/2, including three actually executed
source/fixture/adapter guard mutations with named refusal/restored pass. The
previous five compiled dispatch controls and the full earlier native controls
remain documented below; their source and fixture bytes are unchanged.

This is the complete selected Breakout owner/alias cohort, not transitive or
matrix clearance. The ordinary full transitive run after #325 compiles and
executes all four selected packages: Breakout 67/67 pass; shared types report 227
receipts with zero failed owners; shared exchange has five failed owners among
169 receipts; transport has 58 failed owners among 115 receipts. The gate exits
one with 63 failed owners. Transport failures include 32 missing owner/function
contracts, six missing alias contracts, five stale APIs, two raw APIs classified
as typed, nine positional-array admissions, two tagged-object admissions, one
stale positive sample and one unreviewed function-local decoder. These shared
reviews/repairs are separately owned. No foreign classifications, guard changes
or omitted scopes were used to clear them. Exact local failure summaries and
logs stay outside the repository.

# Account lookup, validation and session invalidation

This slice follows fetched integration b46ce8b19a2e67d6add3cf454450997f75150d12.
Three rows map to actual functions: invalidateSession ported; validateAccount and
getAccountCode CHOSEN for owned/typed/lifecycle boundaries with explicit witnesses.
One Node subprocess executes 18 account cases, eight validation cases, three
invalidation cases and owned/callback/lifecycle supplements. Fixed case identities,
input candidate counts/order, full native return/cache/error values and completed
comparison counts are asserted. No Store writes or market precision reads added.

A sub-agent identified a source corpus gap. Removing the second matching account
from multi-first compiled and PASSED the original actual_account_read target at
exit0: selected output did not change. The repair pins candidate counts and order
without changing original fixture values. The same removal now fails
ACCOUNT-SOURCE candidate count at exit101 and restores/pass. This is an injected
source-corpus omission, not a production defect or a native output-collection claim.

Ten production controls and that one source control compile, fail by name, then
restore/pass. Production covers both invalidated fields, validation log/return
selection, account status filtering, first match, cache commit, original metadata,
and consuming-empty userDetails/accounts decoders. Evidence lives in
account-auth-mutation-evidence.txt and the permanent runner. Decoder controls
consume input fully; ACCOUNT-ERROR message observes loss without trailing-element
protection. Full native values remain independently checked after source census.

Source logger mutation changes a returned error object by alias; native logger
arguments and returned error values are owned snapshots, explicitly witnessed and
classified. Session acquisition and concrete class/API wiring remain pending.

# Candle read slice

fetchOHLCV now executes against 28 actual TypeScript main cases plus three
supplements in one Node corpus. The fixed name/count inventory, native per-case
length, complete decoded field equality and completed comparison total are
independent checks. All six numeric output fields use distinct values and bit
observations. NaN alone is canonicalized; signed zero remains exact. Node26,
arm64 and V8 are recorded by each capture. No original fixture values were repaired.

Fourteen production controls compiled, failed named runtime assertions at exit101,
and restored/pass: timestamp scale; open/high/low/close/volume swaps; from/to swap;
endpoint; metadata flattening; empty/subset/extra output; consuming response decoder;
and consuming decoder with both length guards removed. The latter fails
CANDLE-NATIVE decoded fields, independently of serde trailing-element detection.
The normal consuming control fails CANDLE-CARDINALITY independent native count.
Two initial control attempts were not counted: metadata flattening first failed
on RangeError name, so a name-preserving control isolates metadata loss; empty
output initially failed unused-variable compilation, so the corrected control
explicitly drops the consumed mapped vector before returning empty.
Evidence: tests/candles-mutation-evidence.txt and its permanent runner.

Sub-agent review identified encoded-body content-type inference and missing source
lifecycle/UTF8/NaN witnesses; each is repaired. ISO formatting remains an explicit
host dependency, with native forwarding/error propagation checked against actual
Date outputs, not a claim of implementing a second calendar engine. Concrete auth
and API construction remain pending. No new Store writes or precision-step reads.
Integration a298d46d7a was merged via the quotes branch; candle gates are remeasured
at this slice's own final SHA rather than inheriting the prior merge sheet.

# Market and ticker read slice

This slice maps fetchMarkets, fetchTickersForSymbols and fetchTickers. Integration
was fetched and read as dbdf3d15dddc29d9ae885398c62fd0fe2a25acbc and merged on the
separate feat/rust-venue-breakoutprop-quotes branch. PR7332's remote head is unchanged.

The actual TypeScript corpus has 45 main cases, with 193 pinned array counts and
case/handler identities. The eight-test quotes target includes separate source
census controls, native array length checks, full direct native field observations,
and fixed 45-case comparison count. Supplementary witnesses cover owned input
symbols, owned info, post-await fallback timing and snapshots, unpolled/pending
child Drop guards, and source discarded-Promise continuation. Native child release
is measured, not inferred from a stopped polling counter.

Twenty-one production controls compile, fail named assertions at exit101, then restore
and pass. The additional precedence control rejects bulk validation of later rows
before an earlier missing-tier fallback. Both precision fields were mutated independently; five ticker fields
(bid, ask, last, mark, index) were mutated individually. Empty/subset/extra market
and ticker outputs fail NATIVE-CARDINALITY with source census bypassed. Undefined
versus empty output, endpoint, wrapper symbol order, first USD replacement, error
selection, dropped reporter error and reporter metadata flattening have controls.
The metadata control fails QUOTES-ERROR original metadata fetchMarkets. One runner
initially expected the message-selection mutation to reach the ticker group; it
correctly failed the earlier market group. Its expected assertion was corrected,
then failure/restoration were rerun. No fixture values were changed for these
controls. Logs are in tests/quotes-mutation-evidence.txt.

This introduces one paired precision construction, already discriminated by the
original distinct-step source cases; zero step-rounding consumers are implemented.
The old zero-consumer census is historical. Raw numeric info is captured before
Value construction; shared numeric ingress decodes each market row and quote
records. Reporter errors propagate with shared metadata. There are no new Store
writes or base emitter calls. Typed validation and owned/lifecycle boundaries are
explicit CHOSEN notes with actual-source witnesses; no missing-symbol divergence
is claimed. Final scoped gates and review attach to the final committed SHA.

# Combined instrument read (next separate slice)

The fetchCombinedInstruments row has an actual TypeScript witness with 19 cases.
The corpus pins input symbol order/multiplicity and output counts, including
three outputs from duplicate/missing-symbol inputs. Five tests also witness
owned snapshots, UTF-8/record-validation boundaries, concurrent polling, rejection
metadata, both rejected-sibling Drop paths and the chosen host scheduling boundary.
Actual source right-before-left rejection is measured after cross-realm Promise
handlers attach; Rust left-first polling of simultaneously ready errors differs.
The row discloses that boundary rather than claiming chronological Promise parity.

Seven production controls compiled, failed named runtime assertions at exit101,
and restored/pass: first-versus-last lookup; empty/subset/extra output; empty
output with native length guard removed (full-value comparison fails); a
minOrderSize/minOrderSizeIncrement swap; serialized child polling. An eighth
source control removes the duplicate input and fails COMBINED-INPUT instrument
count. Evidence: tests/combined-mutation-evidence.txt. One initial field-swap
control had invalid macro syntax and did not count; the corrected control compiled
and failed COMBINED-NATIVE fields. All Cargo runs are sequential with --jobs 1;
one Node subprocess runs the complete corpus per target process.

This slice reuses existing response codecs and introduces no new deserializer or
precision rounding site. The earlier consuming decoder controls remain attached
to the raw-read slice. Whole-value comparison checks all retained raw fields and
numeric bits; NaN alone is canonicalized, signed zero remains exact. Runtime
captures record Node major and architecture. No fixture values were repaired to
make the field swap discriminate: the two fields started with distinct values.

The final SHA will receive its own scoped gates and sub-agent review; historical
sheets below remain measurements of their original states.

# Raw REST reads and cardinality audit (separate branch)

Candidate branch: feat/rust-venue-breakoutprop-raw-reads. Pinned heads
42da273dc319fbfd2bb2a3daeb3e0295463352a2 and
1a57805040e041b377e6d46cde4e4d54c1500770 are unchanged.
Integration a09ea6489b2012e43dfca621621ae8e916d4d03c is merged locally.
Real tomllib measures 10 ported / 20 divergent / 24 pending, 54 total.
The three added rows are raw instruments, instrument details and portfolio reads.
All 20 divergent rows have canonical CHOSEN notes, witnesses and real Rust targets.

Audit census and mutation proof are separate:

- Four live source corpora contain 56 / 28 / 30 / 45 main cases; explicit main
  comparison counts are 175 / 112 / 120 / 45. Each target invokes Node once.
- Original truncation controls found a real false green: orders.truncate(1)
  survived both formatter and read-wrapper targets. A separate two-valid-orders
  supplement repairs it; original portfolio source fixtures remain unchanged.
- Thirty production empty/subset/extra controls now compile, fail named native
  cardinality assertions with source census bypassed, and pass after restoration.
- Thirteen typed Vec fields have consuming-empty decoder controls: they consume
  all input before returning empty, and fail named native assertions. A fourteenth
  control bypasses both source and native-length guards while retaining the
  mutant; independent decoded-value equality still fails. Opaque cashTransactions
  is separately covered by consuming passthrough mutations in the thirty controls.
- Twelve in-process source-artifact controls cover empty, shortened and dropped
  subcorpora across the four targets, using pinned independent census inventories.
- Six raw-read production controls, four composed-falsy controls and six logger
  controls compile, fail their named assertions at exit101, and restore/pass.
  Evidence logs and reproducible runners are adjacent under tests/.

Cutover enumeration iteration 2 (after callback repair): zero direct Store writes,
zero base/emitter calls, seven Store read sites, three fallible report calls with
propagation. The earlier direct-call census did not cover callback contracts.
Balance and order formatters now propagate original shared ExchangeError through
fallible callbacks; wrappers convert the full error, preserving subscriber IDs
and cause metadata. Six logger controls prove discard and metadata flattening
fail LOGGER-PROPAGATION assertions. No subscriber-ID FINDING or local sidecar is held.

Per-field precision census remains VALID NEGATIVE: no implemented two-axis step
consumer exists. Pending placeOrder amount, limit price and stopPrice require
fractional distinct-step controls when implemented; this is not swap-proofness.

Pre-commit restored-tree validation: 26 tests passed; strict host and wasm clippy
passed separately; scoped fmt passed. One initial test invocation omitted the
required SAFE_CEX_TYPESCRIPT_PATH and failed oracle setup; the corrected invocation
passed all 26 tests. All Cargo commands used --jobs 1, sequentially. No Yarn or
CI rerun was used. Node v26.7.0 / major26 / arm64 is recorded by the live captures.
The final committed SHA receives a separate verification and sub-agent verdict;
these pre-commit measurements are not relabelled as a run against that SHA.

The known KuCoin integration compile failure is outside this crate's scoped gates.
No whole-workspace build claim is made. Extractor source drift is no longer cited
as an inherited caveat; the venue source/index gate checks this venue only.

# Separate portfolio-read branch: merged validation

Branch: feat/rust-venue-breakoutprop-portfolio-reads. Depends on pinned PR7257
at 42da273dc319fbfd2bb2a3daeb3e0295463352a2, which was not advanced.
Fetched integration 710e663156c8a3c43f8d40af2aa2076b960368d8 was merged without
conflicts at 834fac93835da30d370dd3a29193dfbe7754ffc3. Historical local-only and
older gate statements below remain attached to their earlier measurements.

The Store cutover caused three unused-Result errors in portfolio_reads.rs.
Inspection found two additional fixture update calls in portfolio.rs. All five
now assert success via expect, rather than discarding errors. No production Store
write exists in the implemented source: its Store uses are reads. No subscriber-ID
finding or boxed identity workaround is held in this venue; the fetched shared
runtime contains the structural ID field and both notification append sites.

Measurements on the merged source plus these fixture migrations:

- Real tomllib: 10 ported / 17 divergent / 27 pending (54 total).
- All six read controls rerun: named assertion failure at exit101 and restored
  pass for each. Output is byte-identical to portfolio-reads-mutation-evidence.txt.
- Full scoped venue suite: 17 passed, 0 failed (6 parity + 5 formatter + 6 read).
- Strict host and wasm32-unknown-unknown all-target clippy: separate passes.
- Scoped cargo fmt and included native_fields.rs rustfmt: passed.
- Maintained source/index gate: 54 rows / 42 public Rust items, passed.

Every Cargo invocation used --jobs 1, sequentially. One Node invocation per
corpus; no Yarn shim used. The earlier precision inventory remains VALID NEGATIVE:
venue production bytes are unchanged from the reviewed audit. This gate sheet
includes the lifecycle/drop supplement and does not label the prior 16-test run
as covering the merged 17-test state. The current commit adds fixture Result
handling and this evidence; the final SHA receives its own sub-agent verdict.

# Merged-state validation after release of prior hold

The user released the earlier push hold and requested integration `dec68e5490cb`.
It was merged without conflicts at `ba3ac3c94aabea378b19c24de204d37893949614`.
The following measurements were rerun on that merged source; the final evidence
commit changes only this document and the portfolio mutation log. Earlier local-only
and deferred-merge statements below are historical, not the current handoff state.

- Real tomllib: **10 ported / 13 divergent / 31 pending**, total 54.
- All 19 constants/types/utility production controls: named runtime failures at
  exit 101 followed by restored passes. Output matches row-mutation-evidence.txt.
- All 8 portfolio controls rerun on the expanded 27-scenario corpus: named runtime
  failures at exit 101 followed by restored passes. The current portfolio log
  records this complete rerun rather than the earlier seven-plus-one sequence.
- Full venue-scoped cargo test: **11 passed**, 0 failed, with --jobs 1.
- Strict host and wasm32-unknown-unknown all-target clippy: separate passes,
  --jobs 1, -D warnings. No concurrent Cargo processes were launched by this agent.
- Scoped cargo fmt and included native_fields.rs rustfmt: passed.
- Maintained source/index gate: **54 rows / 38 public Rust items**, passed.
- Live corpora record architecture and Node major; one Node process per corpus.
  No yarn shim or shared corpus edits were used.

The merged production state was sub-agent reviewed at ba3ac3c94aabea378b19c24de204d37893949614
with no findings. Final evidence-commit review is recorded in the handoff verdict.
After publishing the reviewed SHA, the remote branch is held again: follow-on
work stays local until the user merges or releases it. There remain 31 pending
rows; this is not a complete exchange implementation.

# First slice evidence

Scope: `safe-cex-venue-breakoutprop` and its four manifest files. Integration
ancestor: `a3d514bec82d6760d4e9e7a27186f992d175c36d`, merged down before implementation.

The seven source response aliases become 22 records with 124 fields. The live
compiler AST and an independent sub-agent census both measured those numbers.
`tomllib` measures 54 rows: 10 ported, 9 divergent, 35 pending. The divergences
record UTF-8 string ingress; response rows additionally disclose discarded
undeclared fields. The 35 pending rows include the actual exchange, API factory,
REST orchestration, writes, and polling subscription/lifecycle implementation.
This slice does not construct a functioning venue.

## Behavioral witnesses

`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`
is required. `tests/oracle.cjs` transpiles actual checked-in TypeScript constants
and both standalone helper declarations. Erased response aliases have no runtime
codec: their AST supplies declared shapes, and actual `JSON.parse` supplies their
numeric ingress. This is not a claim that TypeScript validates those shapes.

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-breakoutprop
--test parity --jobs 1` exercises six tests:

- `native_fields_and_bits`: 56 response cases, each through raw text, owned Value
  and borrowed Value ingress (168 comparisons). Every native field is observed
  directly, independently of serde field names. Unique finite values detect
  swaps. Profiles cover negative zero, negative underflow, positive/negative
  overflow, integer rounding, the least subnormal, and maximum finite f64.
- `constants_and_lookup`: 10 actual TS constants and six tables, including
  inherited `constructor`, `__proto__`, `toString`, and absent lookups.
- `source_helpers`: 19 timeout cases and 11 order-type cases, comparing result
  bits or exact error messages.
- `surrogate_boundary`: seven actual JSON.parse acceptances paired with Rust
  ingress refusals. Ordinary scalar Unicode and astral pairs are in all profiles.
- `helper_surrogate_boundary`: both actual helpers retain the lone surrogate in
  their diagnostic, observed before JSON; shared Rust string ingress refuses it.
- `unknown_field_boundary`: seven cases add fields at every declared record level.
  Actual JS retains them; Rust's typed projection discards them. Opaque
  cashTransactions retains its contents and is excluded from that discard.

NaN alone is canonicalized at the observation boundary; signed zero is retained.
No shared fixture or corpus was created, edited, or regenerated. The corpus is
produced live; the native observation code is checked in and never regenerated
by tests. Unsupported AST field syntax and inherited interfaces refuse.

## Negative controls

`python3.12 sdk-rs/safe-cex-venue-breakoutprop/tests/mutations.py` sequentially
mutates source, requires a failing named assertion, restores exact original bytes,
and requires the corresponding test to pass. The original six supplemental
fail/restore/pass pairs ran (retained via `--supplemental`):

| Mutation | Failure assertion |
| --- | --- |
| Swap `ask`/`bid` serde names symmetrically | `native_fields_and_bits` |
| Decode PortfolioOrder.finalStatus as true | `native_fields_and_bits` |
| Permit minute 60 | `session_timeout_bits` |
| Change RECV_WINDOW | `constant_RECV_WINDOW` (originally `actual_typescript_constants`) |
| Remove inherited-interface refusal | `inherited-field control` |
| Use stock Value visitor for cashTransactions | `native_fields_and_bits` |

The first sub-agent review identified the all-true portfolio finalStatus fixture.
Profiles now vary every boolean, and a permanent census requires both boolean
states at every declared boolean path. The hardcoded-true mutation fails.
The review also required separate justification and evidence for unknown-field
discard; both are now present in the response rows and live witness.

## Scope of structural gates

`tests/manifest_gate.py` runs the maintained TypeScript `extractFile` against
only the four venue files and compares `(symbol, kind)` identities with tomllib.
This measured 54 matching identities. It is not a package semantic typecheck.
The Rust indexer runs against a one-member workspace view under ignored target,
symlinked to this actual crate, with these four manifests and no exemptions.
This checks status, provenance, public-item coverage and mapping resolution; it
is not itself behavioral parity proof.

Host/wasm clippy are separate `--all-targets --jobs 1 -- -D warnings` invocations.
Formatting is scoped with `cargo fmt -p safe-cex-venue-breakoutprop --check`, plus
rustfmt on the included native-field observation file. No workspace test suite,
monorepo test suite, live account, browser or remote exchange was exercised.

## Per-row production mutations

The default mutation run now covers every one of the 17 constants/types rows,
plus both mapped utility rows. It parses the types manifest with tomllib and
requires exact equality between its row names and the mutation inventory.
Each mutation changes production Rust, requires exit 101, the named assertion,
and the Rust test failure summary; compilation failure cannot count as proof.
Original bytes are restored before a fresh passing test run.

Ten constants use `constant_<TS_SYMBOL>` assertions. Six source object constants
also compare their ordered entries to actual TypeScript `Object.entries`, so
reordering cannot hide behind map equality. Seven response aliases use symmetric
serde string-field swaps and fail `row_<TS_SYMBOL>` inside native field observation.
Both utility mutations fail `session_timeout_bits` / `order_type_branch`.
The run output is retained in `tests/row-mutation-evidence.txt`.

The oracle remains one Node invocation per corpus, shared by LazyLock across
all tests within that process. The 19 mutation/restoration pairs require 38
scoped test invocations and 38 corpus invocations, never a subprocess per case.
Every Cargo invocation uses `--jobs 1` and completes before the next begins.

## Local portfolio formatter slice

This slice builds on reviewed remote `e9b14e41de422f178b8f90f6fafb270ff6f98b79`.
It remains local while that remote head is held for merge. Integration
`5b3f63f2ea` is deliberately deferred until the user merges or releases the hold.
The earlier sheet above describes the earlier slice, not this candidate.

Real tomllib measures **10 ported / 13 divergent / 31 pending**, total 54.
Four extracted formatter methods consume shared Store and numeric ingress. Their
CHOSEN boundaries are UTF-8, declared-record validation, closed intermediate/output
enums, and owned info/fallback snapshots. Raw info retains unknown fields and
numeric lexemes via passthrough capture before Value construction. REST I/O,
construction, writes, and polling remain pending.

The portfolio target has five tests: actual_portfolio_formatters,
owned_info_boundary, portfolio_utf8_boundary, fallback_snapshot_boundary, and
inherited_closed_output_boundary.
It executes 27 scenarios through all four ACTUAL TypeScript methods (108 results),
plus four info-alias, four surrogate, two fallback-alias and 12 inherited lookup
witnesses. The latter capture typeof/truthiness before JSON, covering constructor,
toString and __proto__ across position side and order status/type/side.
Constructor-only dependencies are stubbed; unused network helpers throw. Date
conversion is injected, with this corpus run under TZ=UTC. The source utility
createPositionId and source constants are executed rather than copied.

The native observations read decoded fields directly, before serialization.
Quantity/notional, mark/last/entry, filled/remaining and fallback totals differ.
Native ticker NaN is injected separately from JSON ingress in both runtimes.
Only NaN is canonicalized; signed zero remains exact. Both live oracle headers
record Node major and architecture: this measurement used Node v26.7.0, major
26, arm64, V8 14.6.202.34-node.28. No shared feeder was edited.

Seven sequential production mutations failed named runtime assertions at exit
101 and passed after exact-byte restoration. Four cover the mapped rows; three
supplements cover deferred enum refusal versus a later source error, raw numeric
info capture, and NaN ticker truthiness. See tests/portfolio-mutation-evidence.txt.
The runner refuses compilation-only failures and exit 126. An initial raw-info
mutation caused an unused-import compile failure; that was rejected as evidence,
the mutation was repaired, and all seven controls were rerun. Each test process
executes Node once for its complete corpus via LazyLock, never once per case.

Scoped validation: portfolio target 5 passed; earlier parity target 6 passed;
crate formatting passed; host and wasm32-unknown-unknown strict all-target clippy
passed separately, each with --jobs 1. The maintained source/index gate matches
54 identities and checks only this venue. No concurrent Cargo invocations or
Volta yarn shim were used.

The sub-agent review of ad9d6b9619ab29fb2fc9f1ce6e0e32f78d94fc08 found missing
inherited-value boundary evidence. The follow-up adds the 12 pre-serialization
observations above, Rust refusals, ts-bug markers, and three later-missing-leg
precedence cases. One additional production mutation makes __proto__ status
refuse early: portfolio_formatOrders fails at exit 101, then passes after restore.
The original seven controls preceded this test-only supplement; their production
bytes are unchanged. The eighth control's evidence is appended separately.

## Local portfolio read wrappers (after pinned PR 7257)

Four source methods are extracted in portfolio_reads.rs: fetchBalance,
fetchPositions, fetchOpenPositions, and fetchOrders. Their raw-fetch factory is
called synchronously when the wrapper is called, matching the source's prefix.
The future is awaited once; a null response reads the shared Store after that
await. Formatter results preserve None (source undefined) versus empty lists.
Raw request errors are generic E and propagate unchanged; formatter error
conversion is a separate callback and never sees a raw rejection. The test uses
Rc pointer identity to verify this distinction, not just matching text.

This is orchestration over an injected fetchPortfolioRaw hook. HTTP/session
orchestration and the complete Exchange implementation remain pending. Declared
record validation, UTF-8, closed enum fields and owned snapshots inherit the
formatter boundaries and have wrapper-level source witnesses. The source is
transpiled from the actual exchange module. The oracle uses 24 scenarios through
four methods (96 results), 18 inherited intermediate/output observations, four
surrogate cases, four deferred-fallback cases and four pairs of info/fallback
alias observations. Runtime headers include architecture and Node major; NaN
alone is canonicalized. One Node invocation supplies the whole test target.

Six controls failed named runtime assertions at exit101 and passed on restore:
one per row, plus eager hook invocation and post-await fallback snapshot timing.
See tests/portfolio-reads-mutation-evidence.txt. Info/fallback alias and malformed
record supplements were added after those controls; production bytes are unchanged.
The five-test portfolio_reads target passed. Prior targets are rerun at this
slice boundary because portfolio_oracle.cjs now exports its loader/fixtures when
required by the read oracle, while retaining its direct-execution output.

This slice stays LOCAL while PR7257 remains pinned at
42da273dc319fbfd2bb2a3daeb3e0295463352a2. It does not claim to include later
integration changes. Local tomllib is 10 ported / 17 divergent / 27 pending.

Local slice boundary gates: all 16 venue tests passed; separate strict host and
wasm32 clippy passed with --jobs 1; scoped fmt passed; source/index gate passed
54 rows / 42 public Rust items. No push or integration merge was performed.

### Read-wrapper review follow-up: host-driven lifecycle

Review of 17b875e9bf94884289d0a14e28f2bc768eddfcf8 found the async lifetime boundary
undeclared. The four read notes now explicitly choose host-driven polling/drop:
the factory is called eagerly, but a lazy raw future is not started until polled.
Dropping before first poll or while pending stops the continuation, unlike a
discarded source Promise. Detached execution would require an owned Store/task
runtime beyond the extracted borrowed orchestration API.

abandoned_promise_vs_dropped_future runs all four actual TS wrappers, discards
their Promises, settles their raw requests, and records one formatter execution
per method. Rust records zero formatting after unpolled and pending drop, one
raw-future resource release per case, and positive formatting when driven to
completion. The source witness drains one event-loop turn to include cross-realm
Promise assimilation; two microtask turns were insufficient and failed the named
source_abandoned_continues_fetchBalance assertion before that observation fix.

The expanded portfolio_reads target has six passing tests. Strict host and wasm
clippy were rerun separately after adding this witness. Production is unchanged
from the six successful mutation/restore pairs. The prior 11 tests are unchanged
from their slice-boundary passing run; no newer full-suite measurement is claimed.

### Accepted-contract review: concrete response object ingress

The selected native/default census contains 55 owners and seven aliases before
integration of the production binding in PR #193. Independently authored
contracts cover those exact 62 keys. The focused production runner emitted 41
runtime results and 21 explicit classification receipts. These selected results
do not establish dependency discovery, new binding owners or the feature/wasm
matrix. Ordinary scoped discovery initially refused a stale shared Exchange
receiver source pin; PR #263 repairs that pin separately.

All 23 existing record decoders (22 public response records and private
`quotes::Combined`) accepted otherwise valid complete positional arrays before
repair. The executed baseline reported exactly 46 native mismatches: each record's
Value and raw-text full-array cases expected refusal but observed actual decoded
fields. Positive objects, scalar/null refusal and the other runtime cases passed.
Applying object guards through private remote Serde schemas gave zero failures
across the same 62 selected receipts, with unchanged compiler owner/alias counts.
The shared numeric ingress pipeline and custom cash-transaction decoder remain
in place. Native number projections compare binary64 bits, with raw-text cases
retaining signed zero through contract loading.

Each guard then had its own isolated compiled control: remove only the concrete
`strict_decode::object(...)` wrapper, require both named full-array native
observations to fail, restore exact source bytes, and require a passing native
receipt. All 23 removal/restoration pairs passed; compilation failures were not
accepted as controls. The formatted multiline InstrumentDetailsResponse call
initially stopped the control selector before mutation; its whitespace-preserving
selector was corrected and its actual failure/restoration pair then passed.

The controls used fresh real compiler declarations and `runProbes(..., {target:
"native", features: [], jobs: 1})`. For repeated body-only controls, all production
source bytes were checked against that baseline or exactly the named guard
removal, preserving declaration spans and line counts. This reuse conveys no
ordinary discovery or matrix permission. Final restoration compared every venue
production Rust file byte-for-byte with the guarded baseline.

Reproduce an individual control by removing only that owner's object wrapper,
leaving its remote schema intact, and running its production accepted-set probes;
the `nativeObservation` complete-array Value/text cases must report expected
`{"ok":false}` versus observed `{"ok":true,...}`. Restore and rerun. Decoder-field
mutation selectors now target the remote schemas, including exactly 13 typed Vec
fields rather than the duplicate public data-field declarations. Their runtime
corpora and the broader crate gates still need a post-integration run before this
PR is ready.

# Per-crate contract storage integration

After master #279/#286, only this PR's 62 Breakout rows moved to
`accepted-set/contracts/safe-cex-venue-breakoutprop.json`; all 18 inherited master
contract files were verified byte-identical. The deleted flat file was removed.
The post-migration focused native run passed 62 receipts (41 executed runtime
results and 21 explicit classifications), preserving exact compiler aliases.

Two error-response projections now execute their raw-data observations rather
than using effect-only classifications: validate_account distinguishes logged
message from returned msg and truthy fallbacks; quotes::report retains object
messages and propagates logger failure. All nine independently authored cases
pass. The strengthened focused run passes 43 runtime results and 19 explicit
classifications, still exactly 62 reviewed owners/aliases. An initial adapter
build had a local variable shadowing error; it was corrected before execution
and was not counted as a mutation failure.

The full native venue suite passes 56 tests; scoped formatting and parity also
pass (7459 rows, 17189 public Rust items). The ordinary accepted gate was rerun
with #286 and refuses shared TransportFuture's Result-wrapped output against
the current fixed local-future alias probe. That refusal remains visible; the
shared policy owner is implementing finite support. These focused receipts do
not claim a full dependency, feature or wasm accepted-gate pass.

# Production Exchange binding and native registry (2026-10-16)

The existing BreakoutpropExchange constructor now assembles shared js_runtime
class functions/promises over the existing source ports and the real
ProductionHost. All 16 trait methods are called by the constructor E2E;
getAccount, fetchSymbolMaxLeverage, setLeverage, changePositionMode,
changeMarginType, scheduleCancel and fetchPublicTrades retain BaseExchange's
exact source Error("Not implemented") without HTTP. Caller client-order IDs
reject before session acquisition. Private portfolio reads leave Store and
loaded flags unchanged. This adapter uses DXtrade sessions and polling, with no
network websocket; the scripted runtime panics on any dial.

Measured verification:

- Full venue suite, including the existing actual-TypeScript oracles: 70 tests
  passed. Production binding target: 14 passed. Source oracles still load the
  actual mirror source; no TypeScript or original fixture values were edited.
- scripts/binding-controls.py: all 16 independent dispatch changes compiled,
  failed their named production E2E at exit101, then restored source passed
  14/14. These prove dispatch coverage, not complete JavaScript semantics.
- scripts/registry-control.py: removal of only the Breakoutprop table entry
  compiled and failed the actual native registry construction witness; original
  source bytes restored and the native HTTP test passed 1/1.
- http_failure_preserves_axios_code_and_config compiled and failed on lost
  ECONNABORTED metadata before the bridge fix. The fixed production wire retains
  code, config, Axios identity and response alongside original native metadata;
  all 14 binding tests pass. Preliminary fixtures using an allowed network retry
  code did not isolate metadata loss and are not counted as this proof.
- Native registry E2E uses loopback reqwest HTTP for DXtrade login, startup,
  reads, placement and cancellation. It checks the source login body, DXAPI
  session header, independent amount/price rounding and no websocket dial.
  Source formatPositions deliberately synthesizes a flat position for each
  stored market; the integration assertion retains that behavior.

The first native HTTP run exposed missing Axios baseURL/relative-URL joining in
shared transport. That production defect is fixed separately in merged PR182;
its actual-Axios eight-case comparison and compiling regression belong there.
This binding is stacked on that fix and the required shared-limiter registry
cutover in PR164. It preserves additive Coinbase and Polymarket constructors.

Native DXtrade remoteSigner login refuses visibly. The native JS date realm
explicitly uses UTC for offset-less legacy dates; other embedding hosts supply
the required local_to_utc dependency. The Vec<String> native order-result ABI
wraps a scalar source ID, retains scalar array entries and visibly refuses
nonscalar IDs or lone surrogates. Existing strict-codec, owned snapshot and
source helper scheduling choices remain disclosed in their original rows.
Manifests were reconciled row by row, never regenerated.

Final local gates: native venue all-targets strict clippy, native registry strict
clippy and venue/registry wasm32 library strict clippy pass; scoped formatting
passes. Native registry HTTP passes 1/1 and the constructor suite passes 13/13.
Global parity passes with 7,459 rows and 17,108 public Rust items (zero findings).
Layers pass 88/88; the live registry oracle observes 48 ordered names and its
script test passes 1/1. Gate and mutation logs stay outside the repository.


The production polling bridge reuses shared loop_util for the source five-second
fallback, warning deduplication and deferred callback. An infinite interval
returns before scheduling any task. poll_loop_uses_source_fallback_and_disables_infinity
compiled and failed the original immediate callback, then passed after the fix.
It covers omitted, zero, negative, NaN, configured finite and infinite intervals,
and checks callback deferral separately from the sleep deadline. This adds one
binding test; the final binding target is 14/14 and full venue suite is 70/70.

# Binding-integrated construction review

After inheriting #193, the ordinary gate named ProductionHost's private-field
Clone as a new construction obligation, then refused the binding's glob import.
The import is now the ten concrete used types; no scanner exemption was added.
ProductionHost Clone retains reusable runtime/HTTP/provider/transport/date handles
and the same parent attachment slot. Its exact construction review is separate
from all raw API/record contracts.

The integrated full venue suite passes 71 tests, including the new native cloned
host witness: second-account construction rejects before any request and the
original account still logs in with its original credentials. A control replaces
only the derived host Clone with an implementation that detaches the parent slot;
it compiles and fails the named live-account assertion at exit101. Exact source
restoration then passes the same native test. This proves the attachment boundary
without changing production Clone or adding any validation authority.

# Five indexed inherited refusals: native proposal evidence

The fresh compiler inventory indexes fetch_symbol_max_leverage, set_leverage,
change_position_mode, change_margin_type and fetch_public_trades on the production
Exchange implementation. get_account and schedule_cancel are absent from that
indexed owner; no contract entries or unit-input permission are proposed for them.

Each indexed method now has a native observer that calls the actual production
Exchange method for two independently authored cases: empty state and seeded
session/account/cache/balance state with different symbol/numeric/boolean inputs.
The observations retain the exact refusal message/name/default metadata, zero HTTP,
unchanged store/native balance bits/options/session/cache/disposal state, unchanged
clock/tasks/reports and client/socket/login identities, and no store/error events.
The binding target passes all 20 tests. Each of five independent dispatch-target
controls compiles and fails its named native witness at exit 101; restoring exact
production bytes passes that witness.

The five exact proposed adapter expressions also compile and execute all ten cases
in a native example against this library. That temporary example is removed; logs
and proposed policy rows remain outside the repository. This proves the adapters,
not accepted-set policy permission or a complete package gate. The ordinary gate
and the binding-integrated inventory expansion remain separate work.

# Binding-integrated mutation completion and typed refusal adapters

The interrupted original corpus was resumed at its first incomplete control.
All remaining 30 controls compiled, failed a named native assertion at exit 101,
then passed after exact restoration: six default controls, six supplemental,
fourteen consuming decoders, two auth decoders and two candle decoders. With the
previous thirteen complete default controls, all 43 planned controls are proven.
The final consuming/candle controls bypass their separate census/cardinality
checks so decoded native full-value assertions independently detect element loss.
Each corpus verified exact source/test bytes restored; no compiler failure counts
as a negative control. This is the venue corpus, not the global accepted-set
mutation gate.

The first inherited-refusal expressions called concrete fixture methods and
therefore did not meet the multi-entry validator's literal $type requirement.
The corrected expressions name the actual owner and invoke its Exchange trait
methods on the native observer's production receiver. All five exact expressions
compile and execute the ten independently expected native cases. The observer
returns the actual ExchangeError after polling; it creates no policy exception.

# Five new binding owners and expanded lifecycle review

The binding-integrated compiler inventory contains 67 owners/aliases: the original
62 plus binding::decoded, binding::native_call, binding::report, host::PollingBase
and host::ProductionHost. All 61 retained contracts outside the lifecycle owner
remain structurally identical. That owner's entrypoints now retain all nine actual
compiler APIs: its original constructor/listener, two portfolio reads and five
indexed inherited refusals. The separate Clone construction obligation remains.

The generic decoded function executes a concrete production Vec<String> profile
with independent valid/order/Unicode and malformed shape/type cases. native_call
executes actual production reads, an invalid order DTO and unbound dispatch.
Portfolio reads observe native position/order fields and numeric bits, original
auth headers and unchanged store/loading state. Their independent expectations
also match executions of the actual maintained TypeScript formatters.
PollingBase observations materialize real callbacks, reconcile native shared stats
and inspect decoded ticker bid bits. ProductionHost observations exercise changing
clock values, retained/released caller handler ownership, cleanup and zero HTTP.
The report helper alone is classified as a specific native-string telemetry effect;
it projects no raw error data, unlike the separately executed raw-error helpers.
Its direct native witness checks exact worker error metadata and disposal suppression.

scripts/accepted-binding-controls.py compiles seven exact production witnesses
in their owning modules; all seven pass. Five independent behavior controls then
produce their exact named native test failures at exit 101 and restored passes:
decoder rejection, raw dispatch, statistics cleanup, handler ownership and telemetry.
The script injects only test modules for baseline/restored runs and restores all
production files byte-for-byte in finally. The named failure lines were separately
verified for all five controls. The first telemetry expectation assumed a main-thread
runtime; source review showed this maintained clock fixture is a worker, so that
expectation was corrected before any telemetry control. It is not a porting-bug RED.

These native witnesses do not grant shared inherited-refusal policy permission or
clear the ordinary dependency/feature/wasm gates. PR252 remains draft for those gates.

Fresh binding-integrated discovery and default native probing retain all 60 owners
and seven aliases, exact compiler artifacts/expanded alias claims, one separately
reviewed construction owner, and a clean local macro/helper scan. The 67 receipts
contain 66 passes and one explicit lifecycle-owner refusal: the shared policy has
not yet installed its five inherited constant-output entries. No owner is omitted.
The seven direct production witnesses and ten exact refusal cases pass separately;
they do not convert that policy refusal into an accepted-set pass.

The pre-binding finite helper fixture also now implements the two newly required
trait hooks with explicit panics outside its witness scope. A missing-method
compile failure was fixed before runtime probing; it is not a negative control.
All 76 tests in the integrated venue suite pass, and strict native all-targets
clippy passes. Shared policy-table sharding and its ordinary gate refresh remain
pending; the PR stays draft.

# Per-crate reviewed policy storage integration

After inheriting #315, only this PR's ProductionHost Clone row moves into
construction/safe-cex-venue-breakoutprop.json. All 52 inherited master policy
shards remain byte-identical; the deleted flat construction map is not restored.
The direct native runner now uses loadPolicy for construction/default/receiver
reviews. Five inherited-refusal source and fixture prerequisite hashes remain
unchanged. Strict wasm32 library clippy passes.

The post-split ordinary scoped gate still stops at the explicit shared
safe_cex_transport::provider::NetworkFailure Clone/Copy construction review,
tracked separately in #297. The finite inherited-refusal mechanism follows in
#293; after it lands, this PR will add its five reviewed Breakout specs beside
their actual Rust source and native fixture prerequisites. These are outstanding
policy reviews, with no table fallback or classification bypass.

The serialized post-split parity command exits zero: 7,459 rows and 17,204 public
Rust items, with zero invention/status findings. Native all-targets clippy, wasm32
library clippy, formatting and the 76-test venue suite pass. No accepted-set
completion is claimed while the named shared and inherited-method reviews remain.


# Native trait candle reads and polling listeners

The constructed production-host tests in `tests/binding.rs` expose the actual
Exchange trait defaults: four regressions fail before installing the hooks.
The repaired native hooks dispatch candle reads through the existing
`candles::fetch_ohlcv` orchestration and attach listeners to the actual polling
container. Two distinct candle rows are inspected natively, independently of the
ignored request limit. Request body/auth, original HTTP rejection metadata,
polling ticks and cleanup-handler removal are checked at the real host boundary.
An additional invalid-date test checks the original RangeError and absence of a
marketdata POST. Login and account discovery retain their source ordering.

`scripts/stream-binding-controls.py` compiles five independent production faults:
removing the trait hooks, disabling both cleanup closures, returning zero candle
rows, returning only one row, and bypassing shared Date TimeClip. Every fault
fails its named runtime assertion with exit101; exact byte restoration passes
all five binding regressions. The TimeClip fault also fails the actual JavaScript
ISO comparison, and restoration passes that separate target.

The canonical ISO formatter moved from websocket diagnostics into
`safe-cex-num::js_date_to_iso_string`; the websocket wrapper delegates to it.
`safe-cex-num/tests/date_iso.rs` compares exactly1,044 raw-bit inputs and completed
outputs with actual `new Date(value).toISOString()`, including extended years,
nonfinite values, signed zero, fractional milliseconds and both TimeClip edges.
Only four whole-websocket-file cancellation dependency hashes change after
review of the formatter move. Future creator bodies and cleanup guards retain
their existing ownership. The all-features cancellation target passes all seven
tests including explicit discovery export, with211 creators and no open findings;
the override target passes8 tests and binding scanner25 tests. Scoped invention
checks pass54 BreakoutProp rows/136 public items and233 num rows/193 public items.
These counts describe the inspected native scope, not universal runtime parity.


## Session auth queue and retry ordering

Three `auth_pipeline_` tests in binding.rs compiled against master 5cce34378f
and failed at the actual stale-token request outcome: queued invalidation sent a
request instead of rejecting, while queued replacement and network retry both
sent `DXAPI token` instead of `DXAPI replacement`. The native ProductionHost,
Axios interceptor/limiter/retry implementation and real exchange session state
run unchanged; only terminal HTTP and the deterministic timer are supplied.

After moving auth into the native request interceptor, all three pass. A fourth
control preserves overflow URL params and an untouched native Infinity field.
Replacing retained parameter capture with JSON projection compiles and fails
at the terminal `/users/{username}` request; exact source restoration passes all
four. Existing 401 recovery, HTTP metadata, source order payload and the API
source comparison pass, as does strict Clippy for both touched test targets.
The all-features cancellation inventory passes six tests; override inventory,
parity (7459 rows) and TS extractor check pass without unrelated pin changes.
