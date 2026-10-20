# Public startup resilience (second PR)

Active branch: `feat/rust-public-startup-resilience`, created from landing
`9c34020741` after [broker/cancel PR #1347](https://github.com/Tealstreet/tealstreet-sdk/pull/1347)
merged with 40/40 successful CI checks and three posted correctness reviews.
Local first-PR evidence: 2,811 native cases / 222 targets, 590 harness tests,
76 JS host tests, 444 Wasm SDK cases, 44 typecheck / 28 lint / 27 test tasks,
12 public gates and 11 private gates. The failed Hyro steady-CPU sample passed
an isolated rerun at the same source and unchanged gate thresholds.

The previous public checkpoint `bfce1c1012` was cherry-picked as `21c6b8ab38`.
Both integration conflicts preserve the landing's shared-public-feed module
and docs alongside the new startup policy. Broker and cancellation fixes remain
on the landing branch; no stash, rebase, force-push or master change.

## Execute and review

1. Integrate engine policy and all public family migrations; rerun full native
   targets and repair outdated scripted drivers. Independent reviewer audits
   all-malformed responses, paging, caches and safe retry identities.
2. Repair funded private valuation boundaries and SDK readiness. A private
   account with valid markets/account data must not be disposed by the worker's
   ticker-dependent startup timeout. Preserve shared-feed compatibility and
   real quote handshake requirements. Never synthesize prices or readiness.
3. Run engine/venue/host, root, private/public replay gates; review this phase,
   repair findings, open a PR into `feat/rust-rewrite`, post independent
   correctness reviews, and merge only after green CI. Private row tolerance,
   ticker selection and app payload work remain the following phase.

Shared engine policy: independent public read identities retry at 1/2/4/8/16/30
seconds (30 second cap), deduplicate pending retries and reset after success.
Venues rebuild typed requests and signatures and discard failed accumulators.
Malformed public rows warn without logging payloads; private snapshots/ACKs keep
their old strict semantics until the third phase. Supported blank-number
coercions remain public-only. Private reads start after catalogue availability,
independently of ticker read success. Store::ensure_ticker recovers a known
catalogue row only when real validated live data arrives, preserving the REST
loaded flag. Optimized ticker index caches recover through the same path.

## Checkpoint before first-PR gates

All public source changes compile (engine plus all 26 families; family-specific
checks). New scripted regressions include backoff, malformed peers, independent
private startup, MEXC/WEEX zero/last-price fallbacks, and Bitget/BingX/WOO live
recovery (4 tests each). The eight additional families have 12 passing cases plus
Kraken spot. KuCoin spot has its own migration/conformance cases.

Source and fixture repairs are not yet complete. Do not merge this WIP commit:

- WOO private positions/orders depend on tickers; starting earlier must not hide
  positions or calculate losses using a zero placeholder. Repair using native
  data or explicit retained/deferred valuation; test funded snapshots before
  tickers. The first reviewer identified this dependency before landing.
- Root full tests `/tmp/startup-root-full3.log`: only two obsolete OKX expectations
  failed after lazy recovery; both updated, rerun required. Other root families
  and new public cases passed. Engine ensure_ticker test needs final rerun.
- 12-family full tests `/tmp/startup-all12-second.log`: four old fixture targets
  failed (Bitget USDC catalogue fixed; MEXC early mode read; Phemex watchdog timer;
  WEEX public request counts/watchdog). Repair remaining drivers then rerun.
- Additional eight-family checks passed; final numeric narrowing pending full
  rerun. Coinbase all-malformed empty catalogue should retry rather than claim
  ready. TxFlow fixture assumes private startup waits for every ticker. Preserve
  its explicit ambiguous-native-identity rejection (wrong-asset routing risk),
  visible log and read backoff.
- Review all other ticker-dependent account valuations before landing. No
  placeholder may manufacture an unrealized loss or erase native exposure.
- Final phase sub-agent review, all root/native/Wasm/harness/replay gates,
  correctness PR review and green CI remain required.

Third phase is separately planned in `RUST_APP_STOP_PAYLOADS.md` and the private
rows/ticker subscription audit. It has not been implemented or verified.

## Review repairs in progress (2026-10-20)

- `b8b858568f`: independently reviewed ApeX/Bitrue funded valuation fixes,
  five focused regressions and both crates' all-target Clippy passed. These
  explicit TS bugs are documented in `KNOWN_TS_FIXES`; no replay output is
  normalized by this change.
- Public all-malformed source responses preserve existing state and retry;
  a successful earlier product/page must not hide a wholly malformed later
  required source. Good peers continue to commit.
- Public read retries emit warnings so an otherwise usable authenticated
  account is not rejected by SDK validation. Auth/command failures retain
  their existing error behavior. SDK private readiness requires markets and
  actual private snapshots, independently of ticker readiness; public-only
  and shared-feed readiness still require genuine public data.
- Early socket data must survive delayed REST ticker seeds. Field-level
  overlays retain only fields actually received live, so a BBO cannot suppress
  valid REST mark/last data. Backpack must have one owned public socket.
- Hyperliquid discovery retains native positional identities, preserves
  quotes already received while missing contexts retry, and does not mark a
  wholly unusable catalogue loaded. During partial discovery, private reads
  cannot claim an empty account before an authoritative snapshot. Standalone
  spot still retains raw unpriceable holdings with zero notional after a
  complete catalogue; temporary missing contexts preserve prior valuation.

**Explicit scope boundary:** BreakoutProp's authenticated Instruments/Details/
Quotes discovery remains for the third phase alongside its account row audit.
Those catalogue/ticker reads still need the shared retry and malformed-peer
policy before the overall cross-venue task can be called complete. This public
PR must not claim all 26 families have finished that migration.

Independent review reports and test totals remain provisional until the final
source is committed and all native/Wasm/root/replay gates have passed.
