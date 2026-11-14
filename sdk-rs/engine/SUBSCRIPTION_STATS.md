# Subscription statistics

Managed interests own receipt statistics alongside their released ledger. A
subscription does not count as subscribed until its frame passes the final
handoff. Only native data dispatch calls `Cx::record_socket_topic` (borrowed key
fragments), `record_socket_topic_number` (bounded decimal digits), or
`record_socket_symbol_topic` (lowercase-symbol protocols). None grants socket or
write authority. The diagnostic callback context survives consumption of a
snapshot/auth proof but ends when that exact admitted message callback returns.

A reconnect clears current-epoch activity and released state while retaining
stable topic IDs and the last receipt age, so a dropped resubscribe stays visible.
Unsubscribe removes the row at actual release; replacing queued interests removes
orphaned rows. Recording one topic does not refresh another.
JSON route aliases are compiled at subscription setup, including per-product
aliases for batched Kraken subscriptions; no second message parse or per-frame
allocation is required. Private feeds without managed interest entries, including
URL subscriptions and authentication-owned streams, use one implicit private row.
Native variants whose response does not identify a subscription remain unobserved
rather than inventing receipt proof.

Topic labels and implicit private rows are created lazily at publication. Inline
private receipt state preserves traffic received before that first publication.
Managed telemetry rows reserve their exact initial capacity; route aliases share
one sorted engine table. Typed alias decoding runs only for JSON interest keys,
and only the Hyperliquid, Kraken and TxFlow families enable `socket-topic-aliases`.
Receipt handling uses borrowed fragments or bounded stack bytes, without generic
formatting or heap allocation.

SCXF section 25 (`MESSAGE_HANDLER_STATS`, revision 0) carries a snapshot or changed rows:

- `u32 count`, `u32 mode` (1 replaces the complete map, 0 updates named rows).
- `count` rows of `u32 group`, `u32 topic string reference`, `u32 flags`,
  `u32 reserved`, `f64 age_ms` (24 bytes per row).
- Groups: 0 public, 1 business, 2 futures, 3 private.
- Flags: bit 0 subscribed, bit 1 active (a subscribed topic has delivered data).
- `age_ms` is monotonic elapsed time at frame production, or NaN before receipt.

Normal store flushes publish changed topic statistics at most once per second;
subscription changes publish a complete replacement; full
snapshots include them without consuming the stream's publication gate. The
browser reader validates bounds, groups, flags and ages before publishing a
store update. Older readers skip the optional section. The TypeScript edge alone
converts message ages to `Date.now()` timestamps and retains the existing TS
`MessageHandlersStats` entry shape. Frame-fed trading engines apply source
telemetry independently from account/order-state holds.

The TS callback count is one for a subscribed topic with any receipt history,
including a reconnect awaiting its next receipt; never-observed topics keep a
zero timestamp and callback count. Snapshot replacement clears only buckets
previously owned by that applier, preserving separate shared-provider diagnostics.
