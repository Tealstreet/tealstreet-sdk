# Native handler statistics ownership

BaseWebSocket's statistics algorithm must read the registry that dispatches the
callback. A native venue's private registry is not BaseWebSocket.message_handlers.
The recorded Apex public pong reached its callback while the published statistics
reported hadHandlers=false because the Base table was empty.

`install_native_handler_stats` installs a selected-key count reader in the Base
socket. The native registry remains the only callback owner; nothing synchronizes
or copies callbacks. The private reader is shared by record, pong, cleanup and
periodic reconciliation. Own empty lists remain count zero, missing lists retain
the existing source prototype/default behavior, and reader errors propagate before
statistics writes. Weak captures preserve final socket/registry release.

## Native registry census

The census searched venue Rust callback tables and followed their actual binding
record/cleanup calls, then checked the corresponding maintained socket sources.
These five families need this installer:

| Family | Dispatch owner | Installer | Binding witness |
| --- | --- | --- | --- |
| ApexOmni | `ws_public.rs` private `handlers` | `binding/sockets.rs::bind_protocols` | Recorded public pong through real transport/parser; hadHandlers=true |
| BingX | `public_stream.rs::BingxPublicStream.message_handlers` | `binding/exchange.rs` | Live OHLCV dispatch count and last-unsubscribe cleanup |
| GRVT | `public_feeds.rs::FeedRegistry.handlers` | `binding/public.rs` | Source interval aliases, candle dispatch count and teardown |
| Coinbase, CoinbaseSpot | `public_stream_dispatch.rs::PublicStreamReceiver.message_handlers` | Shared `binding/exchange.rs` constructor | Both variants' real candle/book frames, count and cleanup |
| Backpack | `public_listeners.rs::ListenerState.handlers` | `public_adapter.rs` constructor | Real candle/book frames followed by forced reconciliation |

Backpack's old manual statistics write is removed. It records through Base like
the other owners, so later reconciliation does not overwrite a live count with
zero. Source private socket paths in these families record bare protocol signals,
without registering those signals in a separate callback table.

The following lookalikes are bounded exclusions, not additional parity claims:

- Binance, Aster, Bybit, MEXC and WOO's typed owners register directly in their
  Base handler table; their existing path remains canonical.
- BloFin owns public stream handlers, but its maintained public source does not
  record data-topic statistics. It only cleans up subscriptions and handles the
  Base bare pong signal, which has no registered callback.
- Bitunix futures' statistics call is commented out in the maintained source.
  Public subscriptions do not register callbacks in that public table; the
  existing binding explicitly preserves that scaffold behavior.
- BreakoutProp's polling HandlerTable never records callback statistics. Its
  source cleanup cannot create an entry to reconcile.
- JS-field registries are a separate family. OKX's `sync_handlers` mirror is the
  pending live-JS selected-key cutover (#1218), sequenced after the canonical
  handler ABI change (#1193). No JS socket dispatch is changed here.

## Evidence

`safe-cex-venue-apexomni/tests/recorded-pong.json` retains the exact third frame
from tape `c4971a8e5e8435acba6285455f09f518d32a0636fc2c373e3949c1f422f55713`.
The actual production binding test fails on the pre-installer implementation
(false instead of true) and passes with the installed native registry reader.
The original three-frame startup tape is a focused correctness witness; it is
not a full-minute stream/performance gate.

Run `python3 safe-cex-exchange/scripts/native-handler-stats-controls.py --logs ABS_DIR`
from `sdk-rs`, with the normal isolated TypeScript compiler environment. Each
family must fail its named binding assertion under a compiled reader-bypass fault,
then pass after exact source restoration. Do not run concurrent Cargo during it.

The original three-frame bundle SHA256 is
`4acfab66eea4b2e2df457b477f2d1384a8e3dcdea3bb9e286a4cce4be1ba1aaf`.
The full production napi registry build at `1cdf108ee389d811995ae148dcc109ace0598ecd`
(addon SHA256 `b77df9bc7221226c064725c02c6671219c3114644beea152194fa4b0a1638969`)
strictly drains all three frames, with zero pending/unknown/finite/ready/blocking
work. Both fresh native runs and both TS runs repeat their complete two-output
sequences. The pong flag changes from false to true and all output values and
own-property presence match TS. An independent live-diff-lane replay confirms it.

Order-only differences remain OPEN: the two Store top-level key sequences and
all 178 ticker key sequences (89 rows in each output) differ from TS. They are
exactly the same 180 deltas on baseline and candidate, assigned separately to
codex-4ri6pb. `scripts/live-diff/native-handler-stats-proof.mjs` pins every path
and both key arrays before and after. It neither sorts nor normalizes outputs,
and makes no ordered whole-output claim. The runner's overall startup-only
coverage remains incomplete; only this recorded pong value fix is GREEN.
