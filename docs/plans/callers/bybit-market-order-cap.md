# Bybit and Bitget market order quantity caps

Bybit derivatives advertise `maxOrderQty` for limit/post-only orders and
`maxMktOrderQty` for market orders. Bitget linear USDT/USDC contracts advertise `maxOrderQty`
and `maxMarketOrderQty`. Using the limit quantity for a market split could
produce a child the venue refuses as too large, including a reduce-only close.

The common `Market.limits.amount.max` is the conservative minimum of both
native caps. This also keeps the JavaScript caller-ID check from minting one
identity for a market order that must become several children. The displayed
maximum and caller-ID admission are deliberately conservative for limit orders.
Without a caller ID, the native limit formatter still uses the native limit cap;
market children use the minimum. Every split close child retains reduce-only.
A tighter store override remains binding. Absent, invalid or nonpositive market-specific caps retain
the previous limit bound, rather than publishing a zero maximum.

The retained TS Bybit and Bitget linear reference catalogues publish the same
conservative minimum, so replay parity checks the actual metadata policy.
Bitget inverse keeps its legacy `maxTradeNum` bound unchanged: current docs call
`maxMarketOrderQty` base-coin quantity, while existing inverse code and fixtures
interpret native size/`maxTradeNum` as USD notional. Resolving those units is a
separate investigation; this patch does not combine them. Bitget market-on-trigger
algo sizing is outside this ordinary market/limit formatter change.

Both frame-fed page engines already load the public instrument catalogue, so
they learn these private formatting bounds without account reads. Bybit explicit
refresh stages its native bounds with the catalogue and commits only after the
complete walk succeeds. Bitget stages bounds in its discovery result and publishes
them with the successful catalogue. Neither adds requests, retries or credentials.

Controls cover differing market/limit caps, every reduce-only split child, a
market cap above the limit cap, absent market caps, caller-ID split refusal,
Bybit explicit refresh and frame-fed writes. Existing Bybit public metadata now
expects the fixture's real conservative maximum (500 rather than 1190).

The XTZUSDT incident's closes were confirmed fully filled and reduce-only; the
intervening long came from a separate non-reduce limit buy. This cap fix addresses
the independently observed oversize market refusals. Unknown create-batch ACK
settlement, redacted ACK diagnostics and close-outcome UI copy are separate work.

References: [Bybit instrument quantities](https://bybit-exchange.github.io/docs/v5/market/instrument),
[Bybit batch reduce-only semantics](https://bybit-exchange.github.io/docs/v5/order/batch-place).
