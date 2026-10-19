# Bitget futures engine

Sans-I/O Classic v2 public data for USDT-M and USDC-M perpetuals, with native
wire records, streamed REST lists, and frame output through `safe-cex-engine`.

Books require a snapshot before updates. Modern `seq`/`pseq` are parsed as exact
integers whether JSON numbers or decimal strings. The first delta may overlap
the snapshot sequence; later deltas must name the previous sequence. Gaps,
malformed payloads and legacy checksum failures discard synchronization and
resubscribe. A reconnect drops book state and restores active subscriptions.
Each socket sends literal `ping` every five seconds and expires after twenty
seconds without traffic, including while connecting.

Protocol sources for the scripted tests:

- [Classic changelog: sequence metadata replaces checksums](https://www.bitget.com/zh-CN/legacy-docs/classic/changelog)
- [Classic futures market API](https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market)
- [Classic websocket introduction](https://www.bitget.com/docs/classic/websocket/intro)
- The checked-in Bitget `CLAUDE.md` documents SDK mapping, candle quote volume,
  product variants, and client-order-ID contracts.

Private data supports Classic v2 and livenet linear UTA v3. Startup probes
account settings; a later Classic `40085` response promotes the account, closes
the old socket and invalidates older HTTP generations. REST snapshots preserve
newer WebSocket state, terminal orders and recently opened orders. Failed reads
never commit empty account state.

UTA fills use the dedicated execution channel. UTA account topics cover multiple
products, so orders are filtered by category and positions by margin coin and
known market. Combined standalone TP/SL strategies use separate leg identities
which resolve to their shared native strategy ID.

- [UTA fill channel](https://www.bitget.com/legacy-docs/uta/websocket/private/Fill-Channel)
- [UTA order channel](https://www.bitget.com/legacy-docs/uta/websocket/private/Order-Channel)
- [UTA strategy orders](https://www.bitget.com/legacy-docs/uta/websocket/private/Strategy-Order-Channel)

Trading and queries follow in the committed phase plan at
`docs/plans/RUST_BITGET.md`.
