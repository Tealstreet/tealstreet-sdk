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
