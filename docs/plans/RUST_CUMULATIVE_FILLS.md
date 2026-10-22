# Engine-owned cumulative fill preparation

Owner assignment 2026-10-22, after row-admission #1715. Base is actual install
infra #1716 `084af54692420e95fb1f43837c3d6e589c682c4e`, containing actual row WS
#1715 `2972949f896da9449655011d7e1f36a6f01396f3`. Architecture gate: YES,
cumulative-to-incremental derivation and validate-before-commit are cross-cutting.
Build one engine boundary before repairing additional venue implementations.

## Scope and sequence

1. This PR: new engine `execution.rs` financial preparation, additive `lib.rs`
   export, exact existing Bybit decimal subtraction moved into `num.rs` if needed,
   then Bybit/Hyro and Bitget Classic reference consumers and native regressions.
   No engine dispatch/request/signing/host, history/paging, scheduler/startup,
   clock, fixture, harness, manifest, normalizer or workflow changes.
2. Extend the SAME engine boundary for priced-interval and observed-gap policies
   before the remaining consumers: Orderly, Bitunix, Coinbase, WOO, BloFin,
   BingX, Bitrue, AsterDEX and Kraken Spot. Do not claim this first PR finishes
   their migration. ApeX synthetic execution/history progress needs a separate
   compatibility adapter: cumulative orders intentionally never emit public Fill.
3. Persistent row-reporting extension and remaining keyed admission fan-out;
   then the financial snapshot commit/freshness token, per owner risk ordering.
   Signer-family migration waits for both actual shared signer pins.

Read-only inventory is eleven public cumulative-derived Fill crates plus ApeX
synthetic execution/history progress, not twelve public-Fill implementations.
Binance cumulative progress dedups direct last quantities, OKX uses direct fill
size, and KuCoin Spot has an unsubtracted cumulative fallback; these are distinct
policies, not evidence to force prefix-only delta semantics onto them.

## Engine contract (plan before tests/code)

Use private validated `Cursor { quantity, value: Option<f64> }` and pure
`Cursor::prepare(report, arithmetic) -> Result<Option<Prepared>, Rejection>`.
A validated explicit `Cursor::seed(quantity, value)` establishes native bootstrap
progress without emitting anything. `Default` is the empty zero prefix.
`Prepared` exposes positive finite amount/price and the validated next cursor;
only the native admitted delivery path installs it after identity/side/dedup
checks. Preparation never mutates live cursor, native dedup, order state or root
revision. It is synchronous; no asynchronous pending-receipt/freshness API here.

Typed quantity evidence distinguishes cumulative-only delta from authoritative
native last quantity with optional cumulative progress. Typed price evidence
separates cumulative quote difference with native-authorized fallback from a
direct execution price. Engine owns subtraction, quote/quantity division,
positive/finite checks, next-cursor construction and overflow rejection. Missing
quote is not zero; only a genuinely empty quantity-zero prefix permits a zero
quote baseline. Preserve Bybit's exact decimal subtraction and Bitget's binary
subtraction via an explicit arithmetic policy, without lot rounding or new
blanket clamping. A regressing cumulative quote while quantity rises remains a
specific rejection so Hyro retains its existing close/recovery behavior.

No delivery/no valid price means no advanced delivered cursor and no dedup
consumption. Native valid order observations may still update display/protocol
state. Bitget must pin its validated pre-observation bootstrap cursor when first
seeing an execution (including an unpriced attempt), so a later cumulative-only
correction cannot seed itself from an already advanced display order. Such a
pinned seed records prior trusted progress, never the rejected report's progress.
Keep the existing cursor map lifetime; no new TTL, pruning or global dedup map.
Hyro placement observation/terminal queues remain separate native protocol state;
healthy periodic snapshots do not consume delayed live-fill eligibility, while
reconnect bootstrap still seeds historical progress without notifications.

Preserve authoritative last quantities, native price precedence, same-ID retry
and duplicate semantics, valid late fills, recovery seeding, direct UTA topic,
existing execution IDs and retention. No new per-venue delta arithmetic copies.
All supplied and derived financial values used by the prepared delivery must be
finite and quantity/price positive before delivery/dedup/progress commits.

Later adapters must preserve Bitrue's independently priced intervals: delivered
[6,8] cannot suppress a later [4,6]. WOO observed-versus-delivered blind gaps must
not be priced with a single last execution price. Those policies require engine
extensions and tests before adoption, not one universal highest-watermark rule.
No speculative public-Fill change for ApeX; demonstrate any synthetic-history
loss through its real public query contract before proposing a repair.

## Verification and review

First native assertion-RED controls on original source (no compile-failure
claims): Hyro cumulative .4 with value/average/price unavailable then corrected
same .4/value40 emits .4@100 exactly once, followed by .6@110; Bitget Classic
negative fillPrice on stable trade ID emits nothing, then corrected price99
emits .4@99 once. Also prove cumulative-only corrected Bitget eligibility even
when the unpriced observation advanced display state. Decode actual FILLS price
and amount, not just count. Preserve valid partial/terminal/reconnect/periodic
snapshot/quote-regression/late-direct/duplicate controls. Engine tests cover
missing quote, invalid supplied/derived values, overflow, no-op equality,
regression, direct-last precedence and both arithmetic policies.

Run focused engine+Bybit+Bitget+Binance native tests and strict touched-crate
Clippy/fmt/bans with CARGO_BUILD_JOBS=4/CARGO_INCREMENTAL=0, plus root typecheck
and lint. CI alone runs release wasm, size, timing and complete replay suites.
Before proof, freeze exact archived workload inventory from workflow/catalog:
Bybit family, Bitget Classic/USDC and Binance controls, all correctness/performance
indexed runs. Compare every full raw JSON output, excluding only top-level metrics;
keep IDs, ordered diagnostics, requests, receipts and checkpoints. Healthy tapes
must be unchanged; any deliberate behavior change needs exact reviewed evidence,
never broader allowances. Authenticate actual baseline/candidate heads, trees,
artifact IDs/digests and input fingerprints with no fallback.

Independent plan, code/adversarial and final raw-evidence reviews must answer the
architecture gate. Full green exact-head CI remains mandatory. Integrate actual
shared dependencies before proof; use trial only under normal dependency rules.
Fresh fail-closed revert guard and merge are separate calls, followed by actual
parents/tree/owned-and-incoming blob audit and Aimux actual-pin report. No live
capture, local release build or local full replay/load/timing sweep.

Independent plan review at a542ab579a0 CLEAN to start native assertion-RED tests;
architecture YES. Before implementation, make quantity/price pairing structural:
cumulative quote differences are only valid with the corresponding cumulative
quantity interval, never with an authoritative last amount. Validate seed
average-times-quantity and decimal scale/division overflow; failed seeding must
not silently become zero bootstrap progress. Review late direct-fill cursor
behavior separately: existing Bitget overwrites supplied cumulative progress even
when older. A native newer [.4,.8] execution then late [0,.4] and cumulative .8
catch-up control must demonstrate whether prefix rewind re-emits a delivered
interval; any intentional repair needs explicit evidence/scope report.

Three initial native tests compile on unchanged production source and fail at
financial assertions: Hyro corrected .4/value40 emits nothing; Bitget emits a
negative-price Fill; Bitget unpriced observation advances display but suppresses
its same-quantity corrected Fill. Logs /tmp/codex-cumulative-{bybit,bitget}-original-red.log.
No production code has changed at this evidence point.

Owner approved the fourth demonstrated financial correction: direct last fills
still deliver after identity/dedup admission, but a supplied cumulative anchor
becomes max(previous, supplied); absent cumulative uses checked previous+last.
Keep quote only when its quantity anchor stays unchanged, otherwise clear it
unless new matching quote evidence exists. Add duplicate and genuinely higher
cumulative controls. All four defects are real fill-correctness bugs on current
landing (Hyro/Bitget), to be recorded explicitly in the PR/cutover evidence, not
described as behavior-preserving migration. No production code changed before
this approval. Each has its own original-RED/core-GREEN native regression.

Concrete API refinement: `Report::Cumulative { quantity, price }` accepts
`CumulativePrice::Value { cumulative, fallback }` or `Direct(price)`;
`Report::Last { amount, cumulative, price }` only accepts direct price. Thus a
cumulative quote delta cannot divide by an unrelated authoritative last amount.
`Cursor::seed_with_average` validates quantity/value and fallback multiplication;
invalid bootstrap must reject rather than silently default to zero. Native
financial decoding, bootstrap source selection and lifecycle remain native.
Actual signer #1714 ad0e6fa1e42df883b8fe947e6b4c04eda0da2f7d will be integrated
before implementation and final proof, with all incoming files preserved.

Independent code review found two seed/recovery edges. The existing negative
quote recovery is a compatibility control (original source passes); preserve
ValueRegressed classification for a finite negative quote below a known value.
The missing-quote/zero-average recovery test is a fifth real fill-price defect:
original source emits .6@176.66666666666669 instead of authorized fallback106.
Owner explicitly approved this fifth repair: zero average is unavailable, never
a synthesized Some(0); explicit supplied quote0 stays distinct. Scope is now
frozen to these five defects plus same-contract controls; further unrelated
findings go to follow-up. Original tests compiled (1 RED/1 PASS) with byte-for-byte
source restoration recorded in /tmp/codex-cumulative-seed-original-controls.json.

Actual signer integration `782034a45659bde6f8c6f6e566340832b5bb56ac` preserves
all83 incoming file blobs and both complete README row sets. All four initial
native regressions were independently rerun after it and remain assertion RED
on original production source. Baseline replay CI39719358789/head0343e497e0ce
has the exact actual signer tree. Four authenticated archive ZIP digests match
GitHub: Bybit/Bitget/Bitget-USDC/Binance; frozen24 cases/246 outputs/123 Rust,
70 stable implementation/mode groups. Full candidate comparison still pending.

Local readiness:1120 engine/Bybit/Bitget/Binance native tests pass, including all
five distinct defect regressions and negative-quote recovery compatibility.
Strict touched four-crate all-target Clippy, engine/Bybit/Bitget fmt, bans and
root44 typecheck/28 lint tasks pass. Independent plan and revised code/adversarial
reviews CLEAN (architecture YES, engine core first); reviewed controls retain
bootstrap/healthy-poll separation, native dedup/retention and direct UTA behavior.
Full exact-head CI and complete246-output candidate proof are still merge gates,
not claims established by this local readiness receipt.

First CI cycle: frozen66b618ce677 passed all43 applicable checks and expected
browser SKIP; Rust CI39721825203. Authenticated246/246 full raw outputs match
actual signer baseline39719358789,123 Rust,70 stable groups,10 PASS/14 known FAIL
unchanged, only top metrics excluded. This is iteration evidence after landing
advanced, not the final dependency gate. Actual scheduler #1717
`faf799268857f7aed35badba5349b572ef836c4a` integrated by clean porcelain merge
`30c3188f9d139776018f882b6c55d456ac8ba6ee`: all nonshared incoming/owned blobs
and both complete shared README/lib line sets preserved, exact12 owned files.
The same reviewed production changes remain unchanged. Fresh exact-head CI and
complete246-output proof must now use actual scheduler source-equivalent green
CI39721449594/headf1008a2b48da713c5db2bf378d3196b3619b6a8d as baseline.

## Actual reference-core receipt and full fan-out order

#1720 merged `9fc2a7c174adb60494dbba902826ac20a6094f94` at04:52:56Z. Parents
[faf799268857f7aed35badba5349b572ef836c4a,b10fff339b5664d5751e2b84329e45616b8375d0];
actual treee1f633986c70d8e38ad2d7c7480749b64ada54c2 equals the green head.
43 checks SUCCESS/browser SKIP, Rust39723320673, PR39723320702/push39723315915;
1134 local native and strict checks pass. Fresh246/246 raw outputs (123 Rust,
24 cases,70 stable groups,10 PASS/14 known FAIL) match actual scheduler baseline,
only top metrics excluded. Eight replay archives, eight matching CI packages,
24 file hashes, fingerprints and source trees/parents independently verified.
Fresh exact12 guard then separate merge and actual whole-tree/blob audit passed;
all four coordinating lanes received the actual pin and priority released.

The owner explicitly orders ALL remaining public cumulative consumers, plus the
KuCoin Spot fallback, onto the engine path BEFORE row reporting and snapshot
freshness. This is not completed by the two reference adapters. Remaining native
policies: Bitunix, Coinbase(+spot), WOO, BloFin, BingX, Bitrue, AsterDEX, Kraken
Spot, Orderly and KuCoin Spot. ApeX cumulative notifications remain forbidden;
its synthetic execution/history contract stays distinct. Signing fan-out waits
for actual signer prerequisites. Queue after #1720 is clock #1722, history #1719,
then signer prerequisites #1721; integrate their actual dependencies before any
new final proof/merge, preserving every incoming file.

## Fan-out phase 1 plan: observed suffixes and priced intervals

Fresh branch `feat/rust-cumulative-fill-fanout` from actual #1720. This plan is
committed before new tests or source edits. Architecture YES: extend one engine
financial boundary BEFORE replacing additional per-venue arithmetic. Staged
fan-out: (1) engine observed/interval policies and WOO+Bitrue; (2) engine checked
average/quote/rounding evidence and Bitunix/Coinbase/BloFin/Orderly/Kraken Spot;
(3) remaining BingX/AsterDEX/KuCoin Spot quantity/fallback policies through the
same engine path. These are successive small PRs, not permission to leave the
remaining copies indefinitely or start reporting early. Exact later slices may
combine once native contracts are verified; no per-venue workaround copies.

Current phase files: engine/src/execution.rs and num.rs; WOO/Bitrue account.rs
and native account tests; this existing plan and audit/quality ledger. The plan
already has its README index row. Avoid engine.rs, lib.rs, dispatch/signing/hosts,
clock/auth, scheduler/startup, history/paging, harness/workflows, fixtures,
manifest/normalizer and caller API changes. Coordinate before account signer
migration overlap. No live capture, native shadow deployment or invented data.

### Shared evidence and preparation

Add generic engine observed-suffix and priced-interval preparation, not venue-
named helpers. Keep existing Cursor/Report reference contracts unchanged.

Observed preparation takes a validated native observed baseline, a delivered
Cursor and typed direct-price report: cumulative-only evidence, or authoritative
last amount with optional cumulative progress. The engine owns the conditional
previous quantity selection, checked subtraction/addition/maxima and positive/
finite validation. WOO rule: when total exceeds observed, previous=max(observed,
delivered); otherwise previous=delivered. A fresh native execution ID authorizes
last quantity when present; no-ID reports use only cumulative growth. Missing
last quantity with an ID uses the same observed-suffix growth. Return separate
validated public delivery and optimistic position quantity, preserving
min(delivery, new-observed-delta), plus validated next anchors. A public Fill can
exceed the new position delta. Native order observation, IDs/freshness/status,
terminal retention, repair scheduling and position application remain native.
An invalid/unpriced delivery must not advance the delivered anchor or consume
its native ID, even when the valid order observation advances display progress.

Priced-interval preparation owns a bounded union of delivered quantity intervals
and a bootstrap/retirement floor. Native order keys and outer500-order eviction
stay native. Start with exact existing500-interval cap: clamp to floor, subtract
overlaps, append/sort/merge before eviction, then retire the earliest interval
by advancing the floor. Floor suppression is policy, not proof every earlier
quantity was delivered. A higher endpoint must not suppress an uncovered older
interval: [6,8] then [4,6] both deliver once. Pure preparation returns validated
financials and an opaque commit decision; only the admitted native path commits
the interval and quote state. Keep it synchronous, with no host I/O/await or
asynchronous pending token. Choose a compute-only proposal/commit API that
reuses the interval Vec rather than allocating a full clone on every execution;
validate against the current basis before commit if preparation can be separated
by native state changes. General snapshot freshness remains the later phase.

Typed price evidence distinguishes a uniform direct execution price from two
cumulative quote endpoints tied to the same quantity endpoints. Native last
quantity identifies only its own suffix; a last price never prices a blind gap.
Do not use aggregate interval VWAP for a clipped subset without its own evidence.
Keep separate native observed and delivered quote anchors, including valid
duplicate/no-delivery quote observations. Engine constructors perform checked
scale products, cumulative-average products, quote subtraction/division and
financial rounding; native code selects the available protocol evidence only.
Reject nonfinite/negative financial inputs and all invalid derived results before
any delivery interval, quote anchor or native dedup commit. Missing quote is
unknown, never zero; preserve the established explicit-zero distinction.

Preserve WOO binary arithmetic and Bitrue exact rounding locations/order. Bitrue
decimal() is fifteen significant digits via its existing formatting routine, not
Bybit decimal-place subtraction. Move that pure routine into engine num if
needed, retaining native wrappers used by other mappings. Scale conversion and
rounding before interval identity, overlap subtraction and final price/amount
rounding must remain identical on valid data. No new lot rounding or universal
clamping policy; existing interval zero clamp stays explicit and bounded.

### Native evidence, controls and gates

Source-only candidates, NOT proven bugs: WOO currently advances delivered
execution_cumulative before freshness/positive-last admission; try zero/negative
last then corrected cumulative-only evidence. Bitrue derived quote products and
observed quote insertion are not uniformly finite-guarded; try overflow then
corrected evidence. First prove meaningful native assertion-RED on actual source,
then report cause and obtain explicit owner scope approval before repairing a
new defect. Do not manufacture RED or call a source suspicion financial loss.

Preserve existing native controls: WOO REST-ahead delayed fills, public amount
versus position delta, no-ID dedup and blind-gap suffix; Bitrue interval VWAP,
REST quote reseeding, late intervals, invalid last quantities and missing-last
blind-gap refusal. Add engine controls for partial overlap, bridging, reversed
arrival, zero/invalid/unpriced evidence followed by correction, floor clipping,
cap eviction and finite products/division. Preserve same-ID retries, native
retention, ordered notices and delivery independent of display timestamps.

Run focused engine/WOO/Bitrue/Binance native tests, plus Bybit/Bitget reference
controls when shared existing execution contracts change; strict all-target
Clippy/fmt/bans and root typecheck/lint with CARGO_BUILD_JOBS=4 and incremental0.
CI owns all release/size/timing/replays. Freeze exact WOO/Bitrue/Binance archive
workloads before comparison; authenticate real heads, complete indexed runs,
archive digests/ZIP members and matching package fingerprints. Compare complete
raw JSON excluding only top metrics; preserve full IDs, requests, receipts,
checkpoints and ordered diagnostics. Healthy cases stay byte-identical. Any
deliberate fix delta requires exact native/CI evidence and owner review, never
widened allowances. No manifest/comparator/fixture change without that evidence.

Independent plan, code/adversarial, preservation and final raw-evidence reviews
answer architecture YES/engine-first. Full green exact head, actual dependency
integration, fresh fail-closed guard and separate merge remain mandatory, then
actual parents/tree/owned-and-incoming blob audit and immediate Aimux report.

### Phase 1 concrete API checkpoint and original-source evidence

Actual clock #1722 d3e34e6e105 integrated cleanly at7474c644132 before new source
changes, preserving all ten incoming files. Three native assertion controls fail
on unchanged production: WOO zero-last and negative-last same-ID correction each
lose .001@59000 while display remains .006/position .01; Bitrue after10@100 and
REST30/average110, an average1e308 observation replaces quote3300 with infinity,
then suppresses the valid10@130 suffix from40/average115. Logs are
/tmp/codex-cumulative-fanout-{woo,bitrue}-original-red.log; unchanged source hashes
recorded in /tmp/codex-cumulative-fanout-native-original-receipt.json. Owner
explicitly approves WOO and grants standing approval for proven same-contract
financial defects in this helper fan-out; record each actual fix in PR/AUDITS.

Concrete engine API before implementation:

- ObservedProgress::new(previous, cumulative) validates observation evidence;
  quantity() exposes its monotone observed next quantity. delivery_start(delivered)
  owns the observed-versus-delivered conditional. prepare(delivered Cursor, native
  last Option, direct price) returns an optional PreparedObserved with independent
  validated delivery/next cursor, observed next and optimistic position amount.
  Invalid/zero delivery never consumes a delivered anchor; valid native display
  observation can still commit. A missing cumulative with last uses checked sums.
- QuantityInterval::new(start,end) / suffix(end,last,rounding) validate quantities
  and exact endpoint construction. length() supplies the checked native evidence
  coverage bound. PricedInterval::uniform(interval,price,rounding) authorizes
  clipping; PricedInterval::quotes(from Cursor,to Cursor,rounding) binds quote
  endpoints to its full interval and refuses partial clipping with uncovered
  quantities. A fully duplicated report remains an admitted no-delivery proof.
- IntervalHistory::new(floor,limit) validates its initial floor/cap. prepare takes
  &mut self and a validated priced interval, scans without mutation or cloning,
  and returns a small PreparedInterval holding that exclusive mutable borrow.
  Its consuming commit merges/compacts the existing Vec and returns optional
  validated delivery. The borrow makes stale/reused proposals unrepresentable;
  abandoning the proposal has no effect. Native identity/status/side admission
  precedes prepare, commit precedes Fill, no host I/O between them. This replaces
  a runtime basis check, not a general snapshot freshness token.
- ExecutionRounding::{Binary,Significant15} preserves exact native financial
  output rounding locations. scaled_quantity validates quantity/scale/product
  then rounds before interval identity. Cursor::direct_suffix derives a checked
  quote endpoint from an existing known quote (zero only at quantity zero);
  seed_with_average already checks cumulative products. Cursor::value exposes
  only validated quote evidence. Move Bitrue's exact format .14e routine into
  num with its wrapper retained for unrelated mappings. No full quote ledger.

Bitrue native code selects protocol evidence, quote-map source, last-quantity
coverage and delivery status. Engine owns scale/average/suffix math, quote/price
validation and interval subtraction/rounding/union/floor/cap. Native outer500
order eviction, display/terminal timestamps and quote-map retention stay native.
WOO native order observation/ID selection and refresh/position application remain
native; all delivery derivation goes through ObservedProgress. Add partial-
overlap aggregate-VWAP rejection control plus exact direct-price overlap/late/
bridge/floor/cap, abandoned proposal, checked overflow and correction controls.

Phase 1 checkpoint review CLEAN before implementation: the exclusive mutable
borrow plus consuming interval commit prevents stale/reused proposals; current
Bitrue aggregate-VWAP partial clipping is unreachable under its monotone delivered
basis, but the engine explicitly rejects unsupported subsets. Binary last-size
coverage is checked before rounded interval construction. Native code now uses
ObservedProgress for WOO and Bitrue basis selection; native retention/identity
and wire ordering remain separate. The old Bitrue decimal wrapper had no other
callers and was removed after moving its exact conversion into engine num.
Fourteen engine execution tests and all91 WOO/Bitrue account tests pass, including
the three actual original-source regressions. Broader focused native/strict and
exact-head CI/raw replay proof remain pending, not established by this receipt.

The first code review identified one deliberate observation-policy correction:
without cumulative evidence, WOO previously inferred display quantity from an
unpriced positive last, then added it again on the same-ID price correction.
A fourth native original-RED control proves final display .007 instead of .006,
although only one .001@59000 is delivered. The shared helper already avoids that
rejected receipt: inferred last-only progress waits for validated quantity/price;
explicit cumulative observations still advance independently of delivery.
Corrected same-ID receipt produces display .006, position .011 and exactly one
fill; its duplicate changes none. Original byte restoration is verified in
/tmp/codex-cumulative-fanout-woo-unpriced-original-receipt.json. This additional
financial fix is within the owner's standing same-contract approval and was
reported explicitly; it is not a claim that all observation behavior is unchanged.

### Missing delivered quote enrichment checkpoint

Actual history1719 fa89bc0b59c integrated e0a8cb39fcf with all incoming/owned
source blobs and both shared doc patches preserved. Actual signer1721
802e9c8c6e9 integrated46ed8ffcb4a, all26 incoming+9 owned blobs preserved;
actual tree equals greenf8d6. Family signing is unblocked separately, not part
of this PR. Owner grants current WOO/Bitrue/Binance/execution/num dependency hold;
scheduler-only #1723 may land under an explicit clean-trial exception.

The final targeted review proves a fifth native control RED on actual1721:
Bitrue direct z1/l1/L100/ap1e308 delivers10@100 independently of its invalid
aggregate quote; corrected z1/ap100 supplies the exact missing endpoint quote
without another Fill; z2/ap110 should then deliver10@120 but emits none because
delivered quote evidence was not enriched. Original source was byte-restored,
receipt /tmp/codex-cumulative-fanout-bitrue-enrich-original-receipt.json. This is
the planned valid duplicate/no-delivery quote-observation contract, within the
owner's standing approval, not a new quote ledger or fresh quantity admission.

Before implementation, extend Cursor with enrich_quote(observation: Cursor)
-> Option<Cursor>: return the validated observation ONLY when its quantity
exactly equals self.quantity, self.value is missing and observation.value is
known. Never advance/rewind quantity, overwrite an existing known quote or emit
Fill. Bitrue TRADE admission selects its existing basis and valid aggregate quote;
the helper can fill missing delivered quote evidence at that exact endpoint,
independent of zero-length interval delivery. Existing quote selection ensures
an observed-only basis beyond delivered progress cannot pass quantity equality.
Add engine controls for mismatched endpoints, existing known value, explicit
zero quote and absent evidence; keep original native correction/duplicate
financial assertions. Freeze this slice after these five controls/four financial
bug classes; broader findings go to the next phase unless they block this API.

Enrichment API and revised code/adversarial checkpoints independently CLEAN.
The fifth native control is GREEN with the shared missing-only, matching-endpoint
operation; known/explicit-zero values, absent evidence and quantity mismatches
remain unchanged. All five original-RED controls now have current-GREEN evidence.
Frozen replay suite:13 cases/134 outputs/67 Rust/38 implementation-mode groups,
SHA617f4fa116796af40e8745a28b2045bcb17b30e2c17df0425ac43feb9eba8d60;
WOO2,Bitrue2,Binance9. Authenticated predecessor Rust39726413024/headf8d6cc2
matches actual signer802e9c8 tree, all134 raw bytes bound to live archive digests,
6 PASS/7 known FAIL. Current full comparison and exact-head CI remain pending.

Final local readiness on actual signer/history/clock dependencies:1484 combined
engine/WOO/Bitrue/Binance/Bybit/Bitget native tests pass, including all five actual
original-RED/current-GREEN controls and the original five reference-core controls.
Strict all-target six-crate Clippy, fmt and bans pass; fresh root44 typecheck and
28 lint tasks pass after incoming signer SDK changes. Enrichment code/adversarial
and both actual integration preservation reviews are CLEAN, architecture YES.
No more source changes are planned for this slice unless final CI/proof finds a
blocking defect. Full exact-head CI,134 complete raw comparisons plus matching
CI-package fingerprints, final independent evidence review and separate guard/
merge remain mandatory. Scheduler-only1723 exception is scoped to its actual
read_scheduler.rs+plan diff and conflict-free trial, not a blanket source waiver.

### Refreshed dependency cycle before landing

The first3c2a979 head completed43 applicable checks SUCCESS/browser SKIP and
134/134 full raw outputs equal (67 Rust,38 stable groups,6 PASS/7 known FAIL),
plus six matching CI packages/18 file hashes. Initial Rust39728909641;
proof SHA256cccd23b98bc68debf36aeb21d88d53798de1cf272aa6309bc22d984323a3e659.
This is historical iteration evidence after landing advanced, not the merge gate.

Actual history #1724 22550c4050c33a2f0c0b5bd84b26c027883650e0 changed shared
history/paging and Binance. Owner confirmed real integration was required and
renewed the relevant source hold. Clean porcelain integrationb751e95187fc keeps
all11 nonshared incoming and7 nonshared owned blobs, plus both shared documents'
signed patch sequences. The financial implementation remains identical to3c.
Actual scheduler #1723 08b9c12f4861237f474578928e8ab606da1e1236 then arrived
before the new push. It was also cleanly integrated99d0cf1b5cb1, preserving all
nine owned and both incoming blobs, so candidate CI packages match the exact
commit under test. Its only delta is read_scheduler.rs and its existing plan;
both blobs equal independently reviewed green2a0a1115db3e. No signing or other
financial source edit was introduced by this refresh.

Owner-approved scheduler independence permits retaining actual history22550 /
green56300f5f94bd830d32f67e7155ab6f1f88e5e919 /Rust39728501722 as the frozen
baseline. Candidate source changes since that baseline explicitly enumerate
nine owned files PLUS the two integrated scheduler dependency files; the PR
diff against actual scheduler08b9 remains exactly nine. Proof tooling pins the
dependency's actual parents [22550,2a0], exact two-file delta and both immutable
blobs, and proves the candidate contains it. Every CI package must still match
its own tested head's19-input fingerprint, with no artifact fallback. Compare
all134 full outputs against this baseline; no narrowed suite or new exclusions.

Fresh1509 combined six-crate native tests, strict six-crate all-target Clippy,
fmt and bans pass after both actual integrations. Root44 typecheck/28 lint remain
passing with their TS inputs unchanged. New baseline all134 outputs and three
matching CI packages authenticated; input fingerprint
4c5563177dfe72b68f7236071197db7e8ba1ea90e828783b654dced427b2249e.
Fresh exact-head CI/full candidate proof and final independent preservation/
evidence reviews remain required before a separate guard and merge.


## Fan-out phase 2: cumulative quote admission (plan before implementation)

Fresh branch from actual phase-1 merge #1725
`64bc8d60147b1b1063dc34dfe7af5d6f5ed87e26`. Architecture gate: YES.
Bitunix and Coinbase(+spot) still derive cumulative quantity/quote deltas in
account adapters. Extend the existing engine path before their adoption; keep
all native identity, bootstrap, metadata repair, stream retention and scheduling.
No signing, dispatch, clock, scheduler, history, host or harness source changes.

Add `Cursor::prepare_average(quantity, average: Option<f64>, fallback:
Option<f64>, arithmetic) -> Result<Option<Prepared>, Rejection>` to execution.rs.
A supplied average binds a checked average-times-cumulative-quantity quote.
Without an average, an explicitly authorized direct price binds checked
previous-quote + delta-quantity * price; a missing historical quote remains
missing, never fabricated zero. Subtraction/division/next-anchor construction
then use the same existing Cursor preparation. Validate only pricing actually
used; a no-growth report remains a no-op and regressing quantity remains an
explicit rejection. No fallback across an invalid supplied average or overflow.
Do not change the semantics of existing `CumulativePrice::Value` consumers.

Add `Prepared::rounded(ExecutionRounding)` to validate rounded amount/price
while retaining the exact unrounded next anchor. Reuse Significant15 (the exact
existing `.14e` parser), then remove Bitunix's local clean_float copy. Bitunix
still selects its native positive average or authorized explicit/parent price,
retains separate pending-execution metadata, and commits the next tuple only
after a valid side and prepared delivery. REST/recovery seeding and TPSL paths
remain unchanged. Coinbase uses no fallback and binary arithmetic, retaining its
missing-baseline versus invalid-value diagnostics and poll policy, unknown/live
bootstrap, terminal precedence, stream totals/values and bounded FIFO. Its
admitted live quote comes from the prepared anchor rather than recomputation.
Any baseline-only path that needs checked average binding uses the same engine
method or seed_with_average; do not let a rejected delivery advance a baseline.

First tests are compatibility controls on unchanged production source, plus
engine tests introduced with the API implementation: Bitunix mixed historical
average and direct-price fallback, later average reconciliation, invalid-price
same-quantity correction and duplicate; Coinbase nonuniform cumulative average
prices/quantities for INTX and Spot, missing quote recovery, no-fill snapshot,
invalid cumulative quote followed by correction. Existing native replayable
controls must continue to pass. A newly demonstrated same-contract financial
defect gets its own original-RED/core-GREEN assertion and explicit PR/AUDITS
record under the owner's standing approval; unproven source suspicions are not
claimed as repairs. All changes outside this financial contract are deferred.

Run engine+Bitunix+Coinbase+Binance focused native tests, strict all-target touched
Clippy/fmt/bans and root typecheck/lint with CARGO_BUILD_JOBS=4. CI owns all release
wasm, size, timing and complete replays. Freeze Bitunix/Coinbase(+spot)/Binance
workload inventory from the current workflow/catalog; authenticate both archive
IDs/digests and exact source fingerprints without fallback. Compare all indexed
raw outputs except top-level metrics and all ordered gate failures/fixers;
public timing is the ordinary CI gate. Any deliberate raw difference requires
exact documented evidence, never an allowance expansion.

Independent plan, API/code/adversarial, preservation and final artifact/body
reviews precede full-green exact-head CI, separate fresh guard and separate
merge. Integrate actual shared dependencies before final proof, including the
queued #1726 request-stamp core when landed. Notify the signer lane before
account.rs overlap; Bitunix balance scheduler adoption remains separate from WS
execution admission. Later BloFin/Kraken Spot then BingX/AsterDEX/KuCoin Spot and
Orderly cumulative consumers precede reporting and snapshot freshness work.

Independent phase-2 plan audit at fbd4e288b4a is CLEAN to start native controls.
Before API code: Bitunix keeps `(previous_quote + delta*fallback - previous_quote)
/ delta`, then Significant15 on emitted price/amount only; next anchor is
unrounded. Coinbase equal quantity is only a Fill no-op: it still refreshes a
checked equal-endpoint quote and native terminal/display state. Preserve its
separate missing historical quote warning/poll versus invalid incremental value
warning/poll; never reconstruct the former from the current average. Add equal-
endpoint average correction and non-unit INTX quantity-scale coverage. Engine
preparation errors must never consume native execution eligibility.

Phase-2 implementation extends the engine first and adopts it in Bitunix and
Coinbase live fills. Three engine controls cover historical average/direct quote
mixing, rounded outputs with exact anchors, and invalid/missing/overflow/equal/
regressing evidence. Two native Coinbase controls cover cumulative VWAP and
equal-endpoint correction across Spot, INTX (unit and non-unit contract metadata)
and CFM (non-unit quantity conversion), plus invalid incremental value retry and
duplicates. These pass on unchanged production source before implementation:
compatibility evidence, not new production-defect claims. Initial test fixture
omitted Spot sizing provenance and was corrected with typed base-size metadata;
that fixture failure is not an original-RED production claim.

Independent API/code/adversarial review is CLEAN; architecture YES. Focused
engine/Bitunix/Coinbase/Binance native suite passes 1,020 tests. Root typecheck
44 tasks and lint28 pass (cached). Full CI and authenticated raw/package proof
remain required before merge. No local release build or replay sweep was run.


## Fan-out phase 3: BloFin validated prefix and output preparation

Fresh branch from actual phase-2 #1729
`6e10b0e8a23dbb8d34f761e8199a83c6068fadb1`. Architecture gate: YES.
Move BloFin's cumulative prefix checks, capped-decimal arithmetic and public
rounding into the same engine execution boundary before migrating its consumer.
Keep this PR to engine execution.rs/num.rs, BloFin account.rs/native account
tests, and these existing audit/quality/plan docs. No signing/header, history,
clock, scheduler, host, harness, fixture or manifest change. Kraken Spot remains
the next separately planned consumer: its rejected-price/cumulative correction
needs a native RED and explicit retention/seed design before any new ledger.

Add explicit compatibility policies rather than alter current consumers:
`Arithmetic::DecimalCapped20` preserves native max-input-decimals capped20,
round-away-from-zero subtraction; `ExecutionRounding::ScaledSignificant15`
preserves BloFin's existing decimal-factor/round/divide conversion. Engine owns
checked factor/intermediate/result finiteness and positive public amount/price.
Preserve current Binary/DecimalRounded/Significant15 behavior and exact scaling
order. Checked output preparation scales a validated amount in native contract
units to public base units, rounds at the same native point, and retains its
unrounded contract-unit quote/quantity anchor. No market lot-size rounding.

Add a pure shared prefix preparation API (concrete shape reviewed before code)
that distinguishes no-op/regression, missing-quote enrichment at equal quantity,
valid forward quote observation, and invalid quantity/quote progress. A positive
quantity cannot carry zero/negative/nonfinite quote; a missing quote remains
missing. Strict positive delta-quote checks use the selected arithmetic policy.
BloFin retains its own explicit historical suppression, quiet unknown-prefix
seeding, startup buffer/census ordering and 500-entry retention. The engine does
not infer session status, repair behavior, timestamps or public Fill eligibility.

Adopt checked prefix admission in all execution_prefix consumers, then replace
BloFin's local delta/quote division and output conversions in emit_execution
with engine preparation. No delivered progress may commit until price division,
contract scaling and public rounding all pass. A missing historical quote may
still refresh the baseline without a Fill exactly as native policy specifies;
historical unpriceable progress remains a quiet floor and never emits history.
Equal-quantity corrections only enrich a missing quote, not replace known quote.

Before implementation, add separate assertion-RED controls on unchanged actual
production source: (1) known q1/value1 then live q1.5/average1e308 has a finite
cumulative quote but overflowing incremental price, and must emit no invalid
Fill; (2) after discarding the old invalid output, a corrected same q1.5/average
100 receipt must still emit the .5-contract suffix at price298 once. Use real
native FILLS decoding and native contract-size scaling, not just fill counts.
These are source hypotheses until the assertions reproduce them. Also preserve
startup/history/missing-quote/repeated/terminal controls and test tiny/huge
rounding/scale boundaries through the engine. Any demonstrated same-contract
financial repair is covered by the owner's standing approval and is recorded
individually in the PR and RUST_AUDITS; outside-contract findings are reported.

Run focused engine/BloFin/Binance native tests and strict all-target Clippy/fmt/
bans with CARGO_BUILD_JOBS=4, plus root typecheck/lint. CI owns release wasm,
size/timing and full replays. Before final proof integrate actual provider seam
and any other shared inputs; fresh freeze the full BloFin/Binance CI inventory,
archive IDs/digests, tested-head fingerprints, all indexed raw outputs except
metrics, and all ordered gate failure/fixer arrays. No fallback or allowance
expansion. Independent plan/API/code/adversarial and final artifact/body reviews,
full green tested-head CI, separate guard/merge and actual merge audit remain
mandatory. Continue Kraken Spot and the remaining cumulative consumers before
row reporting and snapshot freshness.


Phase-3 native evidence checkpoint: both controls compile and fail on unchanged
actual6e10 BloFin account.rs (blob ea9e5aee940676ec9d3fe109329fac34a1dd6bb0).
The first decodes `(inf, 0.0005)` from real FILLS after q1/value1 then
q1.5/average1e308. The second separately drains that invalid receipt and proves
the corrected same-quantity `(298, 0.0005)` suffix is lost. These are two controls
for one demonstrated final-output-validation/consumed-eligibility defect class;
tests committed36bf7cee998 precede implementation. Standing same-contract owner
approval applies; the financial findings were reported before production edits.

Concrete API checkpoint before implementation:

- `Cursor::prepare_prefix(previous: Option<Cursor>, quantity, value:
  Option<f64>, arithmetic) -> Result<Option<Cursor>, Rejection>` validates the
  supplied quantity/value first, rejects explicit zero quote at positive quantity,
  and returns no change for valid regression/duplicates. Equal quantity only
  enriches missing value. A forward report with a known previous quote requires
  a supplied quote and a strictly positive checked quote difference. A missing
  previous quote stays explicit and may seed observation progress without Fill.
  This does not alter `Cursor::seed`, whose existing explicit-zero semantics stay.
- `Arithmetic::DecimalCapped20` delegates the exact former BloFin subtraction
  into num.rs; existing Binary/DecimalRounded are untouched. The arithmetic must
  retain native capped-decimal `.round()` behavior, without new lot adjustment.
- `ExecutionRounding::ScaledSignificant15` delegates BloFin's existing decimal
  factor/round/divide conversion into num.rs with checked input/result. Existing
  formatting-based Significant15 is not replaced or reinterpreted.
- `Prepared::scaled(scale, rounding)` prepares positive finite public amount and
  price in the native `delta contracts -> scale -> round` order, keeping its
  unrounded contract-unit next anchor. No state commit occurs inside preparation.
- Keep BloFin's tuple map, 500-entry retention and native lifecycle decisions.
  Its local execution_prefix becomes only a typed engine delegate plus the same
  three diagnostic mappings (invalid quote, missing quote, nonpositive interval),
  with no local arithmetic. Live emit uses checked engine prefix/delivery/output
  preparation before execution_progress and Fill. Historical quiet progress and
  missing-quote recovery use validated prefix observation without requiring an
  unused public price; they never emit historical notifications. Equal-quantity
  enrichment remains quiet. Invalid live output keeps the prior anchor/eligibility.
  Existing clean() callers outside execution output stay unchanged in this scope.

Independent concrete API review is required before source implementation.

Phase-3 implementation and independent code/adversarial review are CLEAN;
architecture YES. Engine prefix observation and checked live output are separate.
BloFin's local prefix wrapper is now an engine delegate plus the same diagnostic
mappings; its clean wrapper delegates the identical relocated numerical policy
without changing nonexecution callers. The two original assertion-RED controls
are GREEN. All812 focused engine/BloFin/Binance native tests, strict all-target
Clippy, fmt and bans pass; root44-task typecheck and28-task lint pass (cached).
Actual provider seam #1731 was porcelain-integrated at1b590e62d702 before this
verification; all seven incoming files are preserved. Remaining shared inputs,
full CI, exact archived raw/package proof and final independent evidence review
remain merge gates. No local release build or replay sweep was run.

Phase 3 landed as #1736 at `288155e8d5be4e63ebcc77f4916514dd14e01df5`
with all 43 applicable checks green and the expected PR-only browser skip.
Independent evidence review verified all 112 full non-metric raw outputs
(56 Rust), 32 stable groups, 11 unchanged ordered gate reports and four CI
packages/12 hashes on exact tested-head fingerprints, without fallback.
Actual frozen Aster prerequisite #1734 was verified before the fresh seven-file
guard and separate merge. No healthy fixture, manifest or comparator change.

### Phase 4: Kraken Spot execution preparation (plan; not implemented)

Fresh branch `feat/rust-cumulative-krakenspot` starts from actual #1736 `288155e8d5be4e63ebcc77f4916514dd14e01df5`. Re-verify source and coordinate the whole Kraken signing family with its owner. This plan is committed before native tests or production edits. Architecture gate: YES, adopt the existing engine execution preparation path; do not add a second financial derivation helper, delivery ledger or new retention policy.

Source hypothesis only: a live trade without `last_qty` computes its amount from the stored order's cumulative quantity. A nonpositive native price currently suppresses the Fill while committing the new order quantity. A corrected report at that same quantity may therefore lose eligibility. First add separate native original-source RED controls proving the lost correction and any poisoned timestamp/terminal state; no production-defect claim without those assertions. Exercise healthy siblings, explicit native execution IDs and both single-frame and same-batch corrections. Keep original source receipts.

Use existing `Cursor::prepare`, `Report::Last` for explicitly supplied last quantity and `Report::Cumulative` with `CumulativePrice::Direct` otherwise, with native Binary arithmetic. Keep `last_price.or(avg_price).unwrap_or(order.price)` precedence. The existing average-price fallback is native compatibility policy, not a claimed interval-price oracle. Validate before staging native times, terminal/provenance, order rows, Fill queue or dedup effects; rejected rows go through the existing engine-backed admission path, preserving their stored copy and healthy peers. Quiet zero-growth/zero-quantity cases remain quiet.

Preserve snapshot/history and nontrade lifecycle suppression, timestamp/cumulative regression fences, accepted-row order within a frame, explicit-last authority, execution-ID/fallback-key identity and 4096-entry dedup retention. A separately accepted `filled` lifecycle row may legitimately advance/terminate the order even if a trade is invalid; do not promise fallback recovery beyond that boundary. Any demonstrated need for a retained independent delivery anchor requires a separately reviewed seed/retention decision before implementation.

After original RED: independent concrete integration/API review, minimal adapter adoption, native engine/Kraken/Binance tests and strict/fmt/bans/root checks, code/adversarial review, docs audit entries only for proven defects, then fresh exact-head full CI. Freeze every current Kraken Spot and Binance workload and compare all authenticated indexed raw outputs except top-level metrics, ordered gate status/failures/fixers, and exact matching CI packages/fingerprints without fallback. No fixture/allowance/comparator changes. Final evidence review, fresh separate guard, separate merge and actual-parent/tree/source-preservation report.

Primary schema read 2026-10-22: https://docs.kraken.com/exchange/api-reference/spot-websocket-v2/executions . Trade and filled are separate event types; cumulative quantity/value and native trade identity are distinct fields. Actual live recapture remains outside this offline financial slice.

Phase-4 native original-source controls at `62c7a01ec21` both fail financial/state
assertions on unchanged actual #1736 Kraken Spot source blob
`0f2e738cf0b818043fd3dc09de9066191fc29576`: a nonpositive-price cumulative
trade loses the corrected same-quantity `(price=100, amount=0.5)` delivery;
an invalid final trade commits `(filled=2, Closed, Market)` instead of keeping
`(0.5, Open, Limit)`. Original source/log receipts are retained. This is the
same validate-before-commit defect class, covered by the standing financial
helper approval. No production change has been made at this checkpoint.

Concrete integration: after constructing the candidate order but BEFORE adding
its native time, terminal id, provenance, row or Fill, prepare live `trade`
financial evidence with `Cursor::seed(previous_order_filled, None)` and
`Cursor::prepare(report, Arithmetic::Binary)`. An explicit `last_qty` uses
`Report::Last { amount, cumulative: e.cum_qty, price: native_price }`; otherwise
use `Report::Cumulative { quantity: filled, price: CumulativePrice::Direct(native_price) }`.
Any seed/preparation rejection adds the row identity to the existing rejected
set and exits that row; existing execution admission handles retention/reporting.
Only a prepared delivery contributes Fill price/amount. Zero/no-growth returns
remain quiet, and snapshot/nontrade lifecycle rows bypass financial delivery
preparation exactly as before. Keep native identity and final dedup staging.
No shared API extension, new ledger or seed/retention reinterpretation is needed.

Phase-4 implementation uses the existing engine API without an extension or
new ledger. All four independently executed original-source assertions are now
GREEN (zero-price, negative-price, retained final row and same-batch correction),
with both healthy-peer orders checked. Two extra compatibility controls keep
zero/no-growth observations quiet without creating a delivery backlog and let
unused snapshot/lifecycle prices retain native behavior. All 844 focused
engine/Kraken/Binance native tests pass; root typecheck44 and lint28 pass with
cache hits; strict all-target Clippy, fmt and bans pass too. Independent
code/adversarial review is CLEAN, architecture YES. No signing, clock, scheduler,
history, caller API/FILLS layout, fixture, allowance or comparator changes.
Independent docs review, actual shared-input integration and full CI/raw/package proof remain
merge gates. The proof must cover both Kraken/Spot jobs sharing the crate plus
Binance; freeze exact current inventories rather than assume a count.

Phase 4 landed through clean replacement #1742 at
07bb7f04d568a07f17a907e89224e511018100f1. All 43 applicable checks succeeded,
with 134 full raw outputs, 67 Rust outputs, 38 stable groups and 13 ordered gate
reports unchanged. Independent evidence and a fresh separate guard preceded the
separate merge. The blocked Bybit stack was excluded; all incoming source/docs
were preserved. Actual shared JSON policy #1744 is included in the next base.

### Phase 5: BingX observed cumulative suffix (plan; not implemented)

Fresh branch starts from actual #1744
2c5c5f6987f35cafc0e272282e54da37c0a66d4f. The ongoing profile probe #1746 and
hosted bridge #1745 remain separate. Coordinate BingX account regions with the
metadata owner before edits; this slice owns only on_order execution preparation
and its financial bookkeeping, not metadata read/coalescing/position settings.

Source hypothesis only: on_order inserts execution identity before selecting
the native Fill price, emits a positive quantity without validating that chosen
price, then commits cumulative/terminal observations. A same-ID corrected event
may consequently lose delivery eligibility. First prove independent original
native assertions for invalid selected price, corrected same-ID/same-cumulative
recovery, retained stored state and healthy subsequent peers. Exercise zero and
negative chosen fallback prices separately where the native decoder admits them.
No production-defect claim or broad mapping repair before original-source RED.

Preserve native side/symbol/order identity checks, explicit trade-ID versus
heuristic time/quantity/price keys, the special cumulative-growth escape for
heuristic collisions, terminal and covered-history suppression when cumulative
is absent, bootstrap seed semantics, 10,000-order/20,000-execution retention,
native price precedence and explicit last-quantity-versus-cumulative behavior.
Unused bad last price must not reject a valid selected native fallback. Quiet
zero/no-growth observations and nontrade lifecycle behavior require controls.
A separately valid lifecycle may legitimately terminate an order; do not invent
a delivery backlog beyond that boundary.

Architecture gate: YES. BingX and Aster's no-ID path both cap a last quantity by
observed cumulative growth; do not add another local subtraction/minimum helper.
After original RED, review a concrete extension of engine::execution that
expresses that direct-price capped-suffix policy without reinterpreting the
existing authoritative Last behavior or cumulative quote/average intervals.
Preparation must remain pure, validate selected financial evidence before any
identity/root/total/dedup/fill effects, retain native gap and seed policies, and
use checked arithmetic when cumulative quantity is absent. Existing helper
adopters must remain byte-identical. Do not choose a new ledger or retention
policy without a separate review/decision.

Then implement the engine policy first with meaningful generic controls and the
minimal BingX adapter; original RED becomes GREEN. Run focused engine/BingX/
Binance native tests, strict Clippy/fmt/bans and root checks, independent
code/adversarial reviews and proven audit/quality updates. Refresh actual shared
dependencies before final CI/proof; no merge jump over the bridge/profile queue.
Freeze all BingX and Binance workloads and authenticate every raw field except
top-level metrics, ordered status/failure/fixer arrays and exact matching CI
package fingerprints/hashes. No fixture, allowance, comparator, signing/clock,
history, metadata API or FILLS layout change. Fresh separate guard, separate
merge and actual parents/tree/preservation report remain required.

#### BingX original-source evidence and reviewed API

On original production source at ce04209c3ae, regression-only 99491884a17
produced four native assertion failures: selected zero/negative price emitted
an invalid Fill; corrected same-ID/same-cumulative evidence lost delivery;
and an invalid terminal execution advanced stored partial state. Two controls
pass on original source: unused negative last price retains a valid fallback,
and accepted zero-growth evidence still consumes its native trade identity.
The production-prefix SHA is unchanged in the original-source receipt.

Concrete API reviewed independently before implementation:
`ObservedProgress::prepare_capped(self, last: Option<f64>, price: f64)` returns
`Result<Option<PreparedObserved>, Rejection>`. It validates last evidence, caps
it by positive cumulative growth when cumulative exists, uses full growth if
last is absent, and delegates the selected quantity to the existing pure
prepare path with a cursor seeded at previous observation. Missing cumulative
uses checked addition. Existing authoritative Last/prepare semantics remain
unchanged. Some(0) expresses native duplicate/lifecycle suppression; accepted
None still permits native identity and quiet observation bookkeeping.

The adapter first looks up identity without insertion, prepares selected
financial evidence, then admits identity/progress/terminal/Fill effects.
An error returns before even the order revision bump. Native price selection,
coverage, terminal suppression, retained identities and liquidation/future-kind
execution handling remain in the adapter. Generic tests cover growth caps,
blind gaps, quiet observations, absent cumulative overflow and invalid price;
independent API review is CLEAN (architecture YES, engine extension first).


Implementation checkpoint: the distinct engine capped-suffix method and BingX
adapter passed 811 focused engine/BingX/Binance native tests, strict all-target
Clippy, formatting and bans. Root typecheck 44/44 and lint 28/28 passed entirely
from cache. Final native-string regression fixtures were rerun against the exact
original production prefix (four assertion RED, two compatibility GREEN), then
restored against the fix (all six GREEN). Both independent source reviews are
CLEAN; caller wording explicitly limits invalid-price rejection to positive
live delivery, preserving accepted quiet bookkeeping. Fresh exact-head CI and
full BingX/Binance raw evidence remain pending; no merge claim.

#### BingX final dependency integration and proof plan

Actual hosted bridge #1745 `5743c485d44` and measured profile #1746
`3f0e3e450b2` are genuinely integrated. Under the overseer's explicit
exact-head-stack authorization, frozen reviewed Bybit #1728
`31035f3f09a71e5fb29d1083c217512972e5f9ec` is also genuinely integrated
for parallel CI. Preservation review is CLEAN: all six original BingX-owned
blobs remained unchanged before this documentation checkpoint, and every
other blob exactly matches that frozen Bybit tree. Architecture gate remains
YES: this is shared engine financial preparation with one bounded adopter.

Fresh final evidence compares the complete 32-suite/110-workload roster
against that exact frozen Bybit PR run, with both synthetic CI checkout trees
and parents explicitly bound to their respective tested heads and the actual
profile landing. All 976 raw outputs, including every full ID and request,
remain compared with only top-level metrics excluded; ordered gate/fixer
arrays and all package hashes/fingerprints remain mandatory. Earlier initial
BingX evidence is historical and is not relabeled. No TS-reference difference
is silently accepted. Merge remains blocked until #1728 actually lands with
the unchanged frozen head as ancestor, full exact-head checks and independent
evidence reviews pass, and a fresh separate guard confirms only these six
owned files against actual landing. Any predecessor head change requires
real integration and fresh CI/proof.

Final proof refresh after the KCS landing trigger: actual #1753
`35949503f7ffab45a942312cfa913da20ce4b71c` is genuinely integrated,
including actual Bybit #1728. The earlier stacked 46d59 CI is preserved as
FAILED solely on KCS CPU and is not final evidence. Use fully green exact
KCS PR run39771903002/head38167 as the authenticated, source-equivalent
baseline (actual merge tree equals that reviewed head), then fresh BingX
exact-head CI/full32-suite proof. Bind the baseline PR synthetic checkout
to [actual1746,38167] and the candidate checkout to [actual1753,new head];
each tree and hard19-input fingerprint must match its own tested head.
All976 raw fields, full IDs and ordered gates remain compared as before.
No source, fixture, allowance or comparator changes accompany this refresh.
Independent six-file preservation review and fresh normal separate guard
against actual landing remain required; no predecessor-stack exception is
needed now that both actual dependency merges are real ancestors.

### Phase 6: AsterDEX observed cumulative adoption (plan first)

Fresh branch begins at actual BingX #1749
`c189ec6e7b326daaa411a7ef8c7b666c88f7ea52`. Architecture gate: YES,
Aster's no-execution-ID last-quantity cap is the same cross-cutting policy as
BingX. Reuse existing `engine::execution::ObservedProgress::prepare_capped`;
no second local cumulative subtraction/minimum policy. Scope is Aster account
execution bookkeeping and native account controls, plus existing audit/quality/
caller/plan sections if behavior is proven to change. Signing/calibration,
listen-key lifecycle, REST reconciliation and history remain native.

Before production edits, run compatibility controls on original source for
last quantity below/above growth, blind gaps and full observed anchors, equal/
regressing cumulative reports, explicit IDs and their duplicates, absent
cumulative evidence, invalid-price retry, legacy flat executionReport and
unknown order kinds. Preserve existing explicit-ID authoritative last fills
in full, including reordered/late unique events independently of displayed
order state. Valid explicit-ID events INCLUDING duplicates must still advance
the independent observation ledger; REST/display seeds are not delivery.
No-ID delivery still requires native cumulative evidence; do not enable the
BingX absent-cumulative fallback there. Preserve positive native price
precedence and separate 10,000-entry execution-ID/observed-order retention.

Concrete API to review before implementation: derive the previous observation
without mutation, construct `ObservedProgress::new(previous, Some(filled))`,
use `prepare_capped(Some(last), selected_price)` for the no-ID branch, and
commit the accepted full observed quantity through the native bounded map.
Explicit-ID delivery admission remains independent; do not gate a valid direct
last fill on an unusable cumulative/display field. The shared helper owns
subtraction/capping/monotonic quantity, while Aster owns identity, storage and
retention. Accepted zero-growth evidence must preserve native bookkeeping.

Investigate negative cumulative observations consuming bounded ledger slots
as a source hypothesis, not a defect claim. Any same-contract financial fix
requires an assertion RED against original production code and GREEN through
the existing shared helper, preserving direct last-fill independence and
healthy peers. The owner has standing approval for proven helper-contract
financial defects; document each in PR/AUDITS. Do not silently clamp malformed
financial evidence or change quiet/retention semantics without proof. If the
existing helper cannot express a proven native policy, review an engine
extension before any per-venue workaround.

Run focused engine/AsterDEX/Binance native tests, strict Clippy/fmt/bans and
root checks. Independent code/adversarial/caller review follows. No local
WASM build/replay/size/timing run. Integrate newer shared dependencies before
final exact-head CI/full authenticated replay evidence, with raw IDs/requests/
receipts/checkpoints and ordered gates intact except top-level metrics.
The current history sequence #1748/#1750/#1754/#1751/#1752 has merge priority;
this slice does not jump it. Fresh separate guard and separate match-head
merge, actual parents/tree/preservation audit and overseer report are required.

#### Aster original evidence and concrete adoption API

Unmodified actual c189 account source SHA256
`80b79f5d772f5832f60c53e3bcd2694a5ba8b4706cc1714e34a26eb4f14f5711`
reproduces one native assertion RED: after a delivered0.5 fill,10,000 distinct
negative cumulative observations evict its healthy ledger anchor and replay
delivers another `(102.0,0.5)` instead of no Fill. Eight original compatibility
controls pass, including10,000 valid quiet observations still applying native
retention, direct IDs with unusable cumulative evidence, duplicate-ID full
observations, blind gaps, legacy messages and native selected-price retry.
This is a proven helper-contract financial defect under standing owner scope.

Concrete implementation to review before source edits: replace private
`execution_progress(&mut self,id,filled)->f64` with
`commit_execution_progress(&mut self,id,progress:ObservedProgress)`. Its bounded
map stores `progress.quantity()` directly; the engine owns validation and the
monotonic maximum. Read previous observation without mutation in on_order,
then call `ObservedProgress::new(previous,Some(filled))`. Only the no-ID branch
prepares `Some(last)` with the selected positive native price; only an accepted
preparation commits observation and optional delivery. Accepted None still
commits valid quiet observations. Explicit-ID delivery stays before and
independent of this observation path; valid cumulative evidence from its
duplicates still commits. Rejected observations skip only that ledger effect;
continue native order-state handling, so no unrelated lifecycle change.
No new engine API, quote policy, retention scheme, logging implementation or
absent-cumulative fallback is introduced. Re-run original RED as GREEN and all
original compatibility controls before broader focused checks/review.

Implementation checkpoint: existing engine preparation and typed bounded
observation commit now serve Aster no-ID delivery and initial REST seeding.
The direct-ID block and native order-state continuation remain unchanged.
Final11-fixture suite was run against the exact original account source under
finally/restore: one duplicate-Fill assertion RED and ten compatibility GREEN,
then all11 GREEN through the shared path. The two additional controls prove
initial REST suppression without hiding direct IDs and later REST reads not
replacing live observations. 799 focused engine/Aster/Binance native tests
and strict all-target Clippy pass. Both independent source/adversarial/caller
reviews are CLEAN; root typecheck44/44 and lint28/28 pass fully cached. Fresh
dependency integration/CI/replay evidence remain pending.


#### Aster final history-stack integration checkpoint

The overseer authorized exact-head preintegration of reviewed frozen history
stack `b1333485b086281a0c9cb23a464c34e06c931d90` for overlapping CI/proof.
Porcelain merge `09c19daa676` preserves all incoming source/test files and the
reviewed Aster account source/tests byte-for-byte. Two documentation conflicts
were resolved exactly as separately approved in #1756 comment6341814528;
marker and per-section checks passed before committing. All incoming history
sections and the Aster financial-admission section remain.

Final merge still requires actual #1754/#1751/#1752 with the frozen ancestry
and reviewed source/tree preserved, full green current-head CI and authenticated
replay proof, independent artifact/control review, and a fresh separate guard.
Any predecessor head change requires genuine integration and fresh checks.
The overseer reserved this final proof window after the history queue lands.
Initial700 evidence remains historical, including its retained strict-false WOO
TS checkpoint-0 variation with both complete pre-change variants independently
authenticated and all488 Rust outputs equal under the standing reference rule.
No exclusions, local WASM builds or local replays were introduced.

The owner reprioritized the shared financial snapshot freshness-token core
immediately after this Aster slice, before remaining KuCoin Spot/Orderly
cumulative adoption and reporting. It needs two different family consumers and
API coordination with RequestContext late reconciliation and the pending
metadata epoch core; no token implementation is included here.


### KuCoin Spot cumulative fallback adoption — 2026-10-23 plan

Start from actual post-gate-sort landing `2cf8c4c91dd`. This PR adopts the
existing engine execution choke point for KuCoin Spot only; Orderly follows on
a fresh branch because its selected-last/average pricing policy needs a separate
review. Row reporting follows both. KuCoin futures, signing and request builders
are outside this slice. The whole-KuCoin signer migration remains held by the
other lane until this slice actually lands.

The current Spot match path emits `matchSize.or(filledSize)` directly. Prove
with an original native regression whether an absent `matchSize` and successive
cumulative `filledSize` values duplicate the already observed prefix. This is a
same-contract financial hypothesis until the unmodified reducer fails. Existing
invalid-price controls already reject before dedupe; do not claim a new defect
there. Architecture gate: yes, this concern is cross-cutting; use existing
`execution::{Cursor, Report, Arithmetic, CumulativePrice}`, not venue arithmetic.

Concrete source scope: `venues/kucoin/src/spot/mod.rs` and `tests/spot.rs`, plus
this plan and the audit/quality/caller ledgers as warranted by proven evidence.
No new engine arithmetic is currently needed. Retain a bounded 20,000-order
map of native `Cursor`s and first-insertion FIFO, independently of the existing
20,000 execution identities and terminal identities. Seed previously unseen
order IDs from individually valid/admitted healthy rows (including healthy peers
in a partially rejected snapshot) only when a current REST snapshot is accepted for
Store publication, never from lookups, stale/failed reads, rejected rows, or non-match WS
display observations. Do not replace a live execution anchor on later REST
reads. Preserve anchors through reconnect, terminal removal and REST absence;
forget them only on explicit bounded FIFO eviction. After eviction the retained
history guarantee ends, just as for native bounded execution identities.

For a match with present `matchSize`, prepare `Report::Last` using the exact
native selected amount/price and optional native cumulative endpoint. Deliver
unique authoritative last quantities before stale/terminal display fences,
without capping by growth or cached geometry. Store its next anchor only when
a native cumulative endpoint exists; absent-cumulative last fills must never
invent a prefix. Prepare an absent-cumulative last against a zero cursor so an
unrelated large anchor cannot overflow and reject its finite native amount.
For absent `matchSize`, use only `filledSize` as the cumulative
fallback endpoint (not `dealSize`), and `Report::Cumulative` with direct selected
price and binary subtraction. Equal/regressed endpoints produce no fill and do
not rewind the anchor. Preserve native presence-based quantity/price precedence,
optional wire bounds/epsilon, side/market admission, and unrounded public fills.

Preserve the identity key verbatim: order/trade ID, otherwise order/raw time/
original selected native amount/price, never derived delta. Prepare without
mutation, then admit identity and deliver before installing progress. Invalid
financial data must consume neither identity nor progress. A quiet cumulative
report must still allow native display processing. Do not reinterpret REST or
non-match display changes as delivered executions.

Before implementation, independently audit this policy. Add meaningful native
original-RED/fixed-GREEN tests for successive fallback suffixes and retained
prefixes after terminal/REST absence; retain existing unique late/terminal and
decimal-roundoff controls. Add controls for direct-vs-fallback precedence,
invalid selected-price correction with the same ID, repeated fallback without
trade ID, stale REST, later REST non-reseeding and bounded retention as feasible.
Run focused engine/KuCoin-spot/Binance native tests with `CARGO_BUILD_JOBS=4`,
strict fmt/Clippy and root typecheck/lint. Independent source/adversarial/caller
reviews must answer the architecture gate explicitly. Do not build local release
WASM or run local replay/timing/size sweeps.

Push for full green CI and authenticated venue+Binance raw replay evidence.
Ordered gate/fixer comparisons require a post-#1762 baseline (actual `2cf8c4c91dd`
or later, or an explicitly authenticated whole-tree-equivalent green head).
No exclusion, comparator, timing threshold, tape or manifest changes. Any
unexpected raw/gate change blocks for review; proven fixture-visible financial
corrections are recorded explicitly. Integrate new shared dependencies before
final proof as lane rules require. Fresh separate guard, then separate merge;
audit actual parents/tree/preservation and report the actual pin immediately.

Plan audit refinements: accepted partial REST snapshots seed individually healthy
returned rows too; retained rejected copies are never new seeds. An authoritative
fill without cumulative evidence cannot identify the interval of a later
cumulative fallback; this migration does not claim to reconcile that ambiguity.
First-insertion FIFO can evict even a still-active order at the documented bound.
Both limits remain explicit; no unbounded history or fabricated prefix is added.

Original native evidence: nine focused fixtures on unchanged production source
produce six assertion REDs (raw cumulative prefix emitted instead of suffix)
and three compatibility GREENs. The RED scenarios cover successive endpoints,
terminal/REST removal, partial-snapshot healthy peers, later REST non-reseeding
and non-match display updates. Original log is retained separately.

Further seed-provenance refinement: a paged read stages raw validated seed
candidates alongside mapped rows, before mapping may substitute newer cached
Store rows. At final current, non-lookup commit, only candidates whose identities
are admitted by the SnapshotBuilder may seed previously unseen IDs. A failed
later page or revision mismatch discards all staged seeds. This keeps display
observations and retained rejected copies out of execution authority.

KuCoin implementation checkpoint: existing engine Cursor now prepares direct
last/fallback reports; raw REST seed candidates retain provenance until accepted
current nonlookup commit. Independent source/adversarial reviews found no source
blocker and required additional lifecycle/identity tests, which are included.
The final19-fixture cumulative suite was run against the exact original source
with finally/restore:13 assertion REDs expose the same prefix-overdelivery defect,
6 compatibility controls pass. Original source SHA256 is
`3c9993f80b66bb4e7265fbae892a857caf900e827fa66306f244b8840f664e47`.
The corrected78-test Spot suite and bounded FIFO unit control pass; the broader
focused engine/KuCoin/Binance run passed919 tests, including the final78 Spot
controls. Strict all-target Clippy/fmt and root44 typecheck/28 lint pass.
Full current-head CI and authenticated post-sort replay evidence remain pending.

### Orderly observed-average adoption — 2026-10-23 plan

KuCoin Spot slice #1764 is merged as actual `49470a1188b` with the separate guard,
match-head merge and complete tree/preservation audit. Start this next slice on
fresh `feat/rust-cumulative-orderly-adoption` from that actual landing. The owner
explicitly approved an additive shared `prepare_observed_average` operation:
preserve Orderly's native arbitrary-last quotient unless an original native RED
proves a wrong Fill. Keep `Report::Last` strict. This is a shared financial choke
point extension, not a venue arithmetic copy. Other lanes' queued history,
RequestContext and signer prerequisite merges keep their existing order.

Source scope is engine `execution.rs`, Orderly `account.rs`, focused native/helper
tests and existing plan/audit/quality/caller ledgers. No signing, requests,
transport, Store freshness, row-reporting, tape, normalization or manifest edits.
The planned API is:

`Cursor::prepare_observed_average(self, quantity: f64, last: Option<f64>,
average: Option<f64>, direct: Option<f64>, arithmetic: Arithmetic)
-> Result<PreparedObservation, Rejection>`.

The result exposes optional `observation() -> Option<Cursor>` separately from
optional `delivery() -> Option<Prepared>`. Use the existing native totals map as
`HashMap<String, Cursor>`; add no second persisted ledger. Native cumulative
field precedence and absent cumulative resolving to literal zero stay in the
adapter. Present last quantity/direct price win by presence, including zero.
Keep binary arithmetic, direct-price precedence and the legacy quote-delta
quotient over the selected amount. Quiet positive cumulative observations may
commit without a Fill. Equal/regressed cumulative never replaces its quote.
All supplied financial fields still validate before any root, stamp, total,
terminal or dedupe effect. Keep exact native error text.

Missing/zero average must remain distinguishable from a trusted quote in the
Cursor, while legacy quotient pricing explicitly uses its existing zero fallback
when quote evidence is unavailable. That representation alone must not change
observable native behavior. A candidate quote-poisoning bug needs native proof:
from zero, cumulative1/last1/direct100 without average emits1@100 but stores zero
quote; next cumulative2/average100 without last/direct emits1@200 rather than100.
Prove this against original production before claiming or fixing it, including a
zero-average variant. A blind gap (cumulative10/last1/direct100) cannot establish
quote1000 and must not be extrapolated as a whole prefix.

If proven, use existing engine `direct_suffix` only for a validated whole-growth
direct execution with a known preceding quote (or the empty prefix). Inferred
quote may install only after native identity/dedup admits the unique Fill; it
must not be consumed by duplicate/quiet/invalid reports. Preserve native
observation-before-dedup order: install the optional observed endpoint first,
then only an admitted delivery may enrich its same-endpoint missing quote using
existing `Cursor::enrich_quote` and `Prepared::next`. This prevents a duplicate
higher-cumulative direct report from inventing quote coverage. Stage all results
before effects; native root/tree all-or-none behavior remains.

First add compatibility fixtures and original-RED financial controls without
calling a nonexistent engine API. Cover explicit last/direct zero precedence,
quiet forward observations, equal/regressed and missing cumulative, duplicate ID
with a higher endpoint, unused regressing average with direct price, sparse older
terminal rows, REST non-seeding, outer healthy peers/root rollback, late unique
fills before display gates, exact binary0.3-0.2, native identity precedence and
4096 retention. Explicit native quotes/averages and gap controls must stay
separate. Review the concrete API/preparation and quote-enrichment policies
independently before implementation; no per-venue financial workaround.

Run engine/Orderly/Binance focused native tests with four Cargo jobs, strict
Clippy/fmt/bans and root checks. Independent correctness/adversarial/caller
reviews answer the architecture gate. Final full green CI and authenticated
all-suite raw proof use a post-#1762 baseline, integrate new shared dependencies
as required, retain strict differences and freshly certify only standing TS
reference classes. No local release WASM build/replay/size/timing sweep. Fresh
separate guard, separate merge, actual tree/parents/preservation audit and
immediate overseer/peer report finish the slice; shared row-reporting follows.

Plan-audit refinement: whole-growth inference requires exact equality to the
selected arithmetic's positive cumulative delta, with no epsilon or gap
extrapolation. It is best-effort: inferred quote overflow/unavailable prefix
leaves quote missing and never rejects an otherwise valid direct Fill. Supplied
average-times-cumulative overflow still rejects at the existing prepare-fill
call sites, including quiet/equal/regressed reports; do not move those checks
into root-only or stale sparse paths that never prepared a Fill. Positive last
reports may deliver at equal/regressed endpoints under native pricing policy.
Post-delivery enrichment only updates a live same-endpoint totals entry and
never recreates an entry removed by native retention/root lifecycle effects.

Orderly implementation checkpoint: additive `PreparedObservation` separates the
strictly forward observation from the uniquely admitted delivery. The existing
totals map stores `Cursor`; native financial calculations now call the shared
operation. Inferred quote enrichment requires Binary arithmetic, exact whole
growth, a known preceding quote/empty prefix and a unique positive direct Fill.
Other arithmetic policies can prepare native deliveries but never infer a quote
using a mismatched binary interval. No changes to `Report::Last`.

The final21 native fixtures on exact original engine/Orderly source yield three
assertion REDs (missing average, zero average, and the same defect in a corrected
root child) and18 compatibility GREENs. Each RED observes1@200 rather than1@100;
the corrected21 cases pass. Root rollback assertions pass on original code and
remain compatibility controls, not a newly claimed root-atomicity defect.
Seven focused engine controls cover observation/delivery separation, exact
whole-growth qualification, known-prefix requirements, supplied-vs-inferred
overflow, direct/last zero precedence, regressed/equal observations and the
non-Binary inference boundary. Full current-head CI/replay proof is pending.

Final focused verification passed851 engine/Orderly/Binance native tests, strict
all-target Clippy, formatting/bans, root44 typecheck and28 lint (root cache hits).
Both independent final source/test/caller reviews are clean. The native match
identity fixture uses actual `match_id`, including a repeated higher endpoint;
its final exact-original rerun preserves3 assertion REDs/18 compatibility GREENs.
The final native fixture SHA256 is
`f0caeffeb5bcce22277f2a636af1706c36c159c4882b8124d9b7cce10f2d09ca`.
