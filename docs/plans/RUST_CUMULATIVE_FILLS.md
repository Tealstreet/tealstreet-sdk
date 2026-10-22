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
