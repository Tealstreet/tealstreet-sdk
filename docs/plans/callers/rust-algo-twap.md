# Rust TWAP core

`sdk-rs/algos/twap` (`safe-cex-algo-twap`) ports the TS worker's TWAP
(`TwapHandler`, `TwapInstance`, `TwapManager`, `TwapPostprocessor`,
`TwapOrderEnrichmentPostprocessor`) as a clock-driven state machine for the
native exchange server. Nothing calls it yet.

## Obligations

- Native server: register one `safe_cex_algo_twap::factory::TwapFactory` per
  account in its `safe_cex_algos::Registry`. The factory owns the account's
  `TwapBook` and timer driver, and emits `twap:stateUpdate` and `log`
  (`[message, "error"]`). It also exposes `active_twaps` and `order_owner` for
  the store's `activeTWAPs` and `extra.isFromTwap`.
- Chaser runtime: implement `factory::TwapChasers` (start with a given id,
  progress by id, and a remove that cancels the open order) over the chaser
  book, and pass it to `TwapFactory::new`.
- Web client: unchanged. It keeps persisting `twap:stateUpdate` and replays it
  as `twap:state` on connect.

## Behaviour callers must know

- Client input is bounded before any slice is built: `size > 0`,
  `duration > 0`, `1 <= ordersCount <= 1000` (the web's `TWAP_MAX_ORDERS`),
  `randomnessPercentage` and `timeoutPct` in `[0, 100]`, and
  `chaserDelay >= 0`. Restored entries must also keep `ordersSizes` within
  `ordersCount`, with finite values. A refusal sends a `log` error and builds
  nothing; a bad restore entry is skipped.
- A slice whose placement outcome is unknown, or whose order ids cannot be
  read, still advances the TWAP, as in TS. Its log says the order may be live,
  and no later slice assumes it failed.
- TWAP ids come from a non-cryptographic generator and are addressable only
  inside their account's book. Never resolve a TWAP id across accounts.
