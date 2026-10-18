# Crypto.com concrete wire string contracts

Seven concrete wire enums already use the maintained String admission followed
by a private Serde remote companion. This slice reviews their actual
native/default contracts without changing decoder behavior:

- CryptocomTradeSide: BUY, SELL.
- CryptocomJournalType: TRADING.
- CryptocomLiquidityIndicator: MAKER, TAKER.
- CryptocomOrderType: LIMIT, MARKET, STOP_LOSS, STOP_LIMIT, TAKE_PROFIT,
  TAKE_PROFIT_LIMIT.
- CryptocomTimeInForce: GOOD_TILL_CANCEL, FILL_OR_KILL, IMMEDIATE_OR_CANCEL,
  POST_ONLY.
- CryptocomRefPriceType: MARK_PRICE, INDEX_PRICE, LAST_PRICE.
- CryptocomOrderStatus: ACTIVE, FILLED, CANCELED, EXPIRED, PENDING, REJECTED.

The literal sets are reviewed against the actual CryptocomTrade, TradeFill and
Order declarations in cryptocom.types.ts. Native tests match the decoded enum
variants independently of Serde names, check existing serialization separately,
and reject every valid-tag externally tagged map through ordinary JSON text,
qualified owned/borrowed Deserialize, and the three numeric frontdoors. Arrays,
null, booleans, numbers, unknown strings and invalid case/space/BOM forms are also
refused. Correct JSON framing whitespace remains accepted.

The per-crate contract shard contains string shapes, exact compiler entrypoints
and direct native variant/ordinal observations through those six decoder paths.
The single-literal JournalType has a second positive raw JSON framing witness;
that does not admit whitespace inside a tag. The maintained native selector
performs fresh rustdoc, closed selected-owner compiler discovery, loadContracts()
and the canonical production probes. All seven owners execute, with zero
classification-only receipts and zero failures.

Seven independent mutation controls remove only each concrete String admission
and pass the original deserializer to its same private remote enum. Every
mutation compiles and fails its named valid-tag tagged-shape rejection assertion;
exact byte restoration passes. A compile failure, unknown tag or malformed map
cannot satisfy a control. The remote companions return the concrete owners and
do not implement an independent Deserialize for themselves. Ordinary private
Wire decoders retain their independent obligations.

From sdk-rs, run:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_string_shapes
python3 safe-cex-venue-cryptocom/scripts/string-contract-controls.py
node safe-cex-venue-cryptocom/scripts/string-contracts.mjs
```

Scope is seven of the original 133 native/default census owners. The remaining
126 owners, six aliases and fifteen construction entries remain owed, along with
any newer discovery, locals, features and WASM contract execution. Compilation
checks for other targets do not establish their runtime contracts. No manifest
status, private-helper exemption or full-package completion is claimed.
