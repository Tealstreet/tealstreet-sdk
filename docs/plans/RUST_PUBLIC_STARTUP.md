# Public startup resilience (second PR)

This phase follows broker/cancel commits through `c9ba8ba2af`, which are kept on
`feat/rust-startup-broker`. Public implementation is parked separately on
`feat/rust-public-startup` while the first PR runs its exact-source gates.

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
