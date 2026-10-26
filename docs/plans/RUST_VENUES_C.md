# Venue findings lane C

Open High/Medium items from `RUST_AUDITS.md` for Coinbase, Orderly/TxFlow,
Lighter, KuCoin, Phemex, Bitget, BloFin, BreakoutProp and the Binance family.
One PR per venue or family into `feat/rust-rewrite`; each fix carries a
regression test. Items another open PR already repairs are re-verified after it
lands, not duplicated.

1. Coinbase trading: release placement reservations on definite rejections and
   bound ambiguous ones; GTC market acts as IOC; stops and TPs reserve separate
   reduce-only capacity and spot reduce-only is a plain order; keep Coinbase's
   rejection messages.
2. Coinbase account: skip unmappable order/position rows keeping the store's
   copy (externally placed brackets/TWAPs no longer block trading); portfolio
   probe advances on non-retryable failures; saved read-only keys sign GETs.
3. Orderly: unscoped cancel-all (stop payloads and TP_SL blocking land in #1353).
4. Lighter, KuCoin, Phemex, Bitget, BloFin (after #1355), Binance family,
   BreakoutProp: remaining mediums in audit order.
