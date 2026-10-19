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

Private data, trading and queries follow in the committed phase plan at
`docs/plans/RUST_BITGET.md`.
