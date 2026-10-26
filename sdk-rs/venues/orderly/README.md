# Orderly perpetuals

One venue crate serves Tealstreet DEX, KodiakFi, WOOFi, Mode, SalsaDEX and
What Exchange through `Options::broker`. Both EVM mainnet and testnet use
Orderly ED25519 keys; `application_id` is the account ID for private requests.
The public-only default is Orderly's common WebSocket application ID.

Broker-suffixed native contracts retain their suffix in SDK symbols, for
example `PERP_XLM_USDC_mythos` becomes `XLMUSDC_mythos`. Private positions
retain distinct cross/isolated IDs. A selected margin mode controls settings
and leverage writes without discarding positions in the other mode.

Funding topics use separate sockets with at most 90 subscriptions each. The
main socket reserves three aggregate topics and supports 97 distinct book or
candle subscriptions in total. Exceeding that capacity logs an explicit error;
unsubscribing frees capacity. Every socket has a handshake timeout and a
heartbeat watchdog.

Caller IDs must be ASCII, at most 36 bytes, without line breaks or a leading
hyphen. IDs on split orders, brackets or conditional orders fail before any
write, because the native API cannot preserve the caller's requested identity
across those operations. Native order IDs remain exact decimal strings.

Standalone stop-loss and take-profit orders accept the app's `triggerPrice`
when it equals `price`; differing trigger and execution prices and opening
conditional orders remain unsupported. Replacing full-position protection
rebuilds only proven `POSITIONAL_TP_SL` / `CLOSE_POSITION` siblings, retaining
the opposite leg. External trailing stops and quantity-based `TP_SL` orders
remain independent and are neither canceled nor rewritten. Mixed or contradictory
parents fail before any write. SDK `cancelAllOrders`
uses both native account-wide DELETE paths for every broker, including when
the latest SDK store frame has no orders.

The shared SDK engine transport preserves signed request bytes through direct
requests, REST proxies and the raw HTTP-over-WebSocket adapter. Intermediary
authorization may add headers but cannot change signed bytes or venue headers.
Hosts apply account socket routing to the venue's declared roles and feeds.
Custom providers requiring upgrade headers fail before startup when the host
cannot send those headers.
Disposal aborts pending requests, including deferred authorization.
Client-order lookup is unsupported; the four history/query methods use
documented native reads.

The independent legacy comparator is test-only under
`packages/safe-cex/src/__tests__/reference/orderly`. Its declared corrections
are in `sdk-rs/harness/known-ts-fixes-orderly.mjs`; no native output is passed
to those corrections.
