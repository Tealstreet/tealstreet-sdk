# Public subscription ordering

The maintained Bitunix `onOpen` records markets in Store order, then `subscribe`
iterates `Object.keys(this.topics)` before enqueuing 20-topic batches. Each topic
key is JSON text beginning with `{`, so object enumeration retains insertion
order. Updating an existing topic keeps its original position.

The native public socket used `HashMap`, randomizing both batch membership and
outgoing bytes. Replace that field directly with `IndexMap`; no sorting or
parallel fallback is involved. The private/futures socket state is unchanged.
The initial 300-market cap, 20-topic batch size and 250 ms queue timing remain
owned by their existing source bodies.

## Proof

`native_identity::public_subscription_order_matches_source_over_repeated_constructors`
builds the production venue and real public socket protocols, with terminal
transport and clock fixtures. The maintained-source oracle executes actual
`onOpen`, `subscribeMarketsToChannel`, `subscribe`, `enqueueSubscriptions` and
`drainSubscriptionQueue` members and reads their constants from the same TS AST.
It observes the exact outgoing JSON bytes, rather than reconstructing expected
frames in Rust.

The 43 markets start BTC/ETH/BNB, then use descending names to distinguish
insertion order from lexical sorting. Each of 32 independent constructors opens
both public sockets and checks every outgoing ticker/price subscription byte
across all three batches (192 frames in total). The original compiled production
body failed on three separate runs at the ordered-byte assertion; the direct
storage change passes all 32 constructors. Source probe setup failures and the
macOS pre-main executable assessment delay are not regression evidence.

The large captured Bitunix tape remains a separate replay gate owned by bsswa2;
this finite proof does not claim full tape output parity. Crypto.com's combined
Store/book ordering has a separate suspected FFI delivery cause and is not fixed
by this map change.
