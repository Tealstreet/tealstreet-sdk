# Bitget futures engine

Sans-I/O Classic v2 public data for USDT-M and USDC-M perpetuals, with native
wire records, streamed REST lists, and frame output through `safe-cex-engine`.

Books require a snapshot before updates. Modern `seq`/`pseq` are parsed as exact
integers whether JSON numbers or decimal strings. The first delta may overlap
the snapshot sequence; later deltas must name the previous sequence. Gaps,
malformed payloads and legacy checksum failures discard synchronization and
resubscribe. A reconnect drops book state and restores active subscriptions.
Each socket sends literal `ping` every five seconds and expires after twenty
seconds without traffic. Private sockets use `engine::socket::Supervisor`: ten
seconds to connect, ten seconds to authenticate independently of heartbeat
traffic, and capped exponential retry with jitter. Rejected login closes and
retires the socket immediately. Repeated login failures retain their backoff;
only stable authenticated traffic resets it. A delayed post-write recovery
retains its recovery hint and waits for any pending authentication retry.

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

Trading uses sequenced jobs for orders, brackets, conditional stops and account
settings. Native amendments send a fresh `newClientOid`; cancellation retains
the original native `loss_plan` or `pos_loss` identity. Ambiguous writes are
never retried. History reads page executions, orders, income and public trades
into the engine query shapes and reject incomplete financial results.

Inverse sizing and UTA bills/trailing remain outside the verified scope.
Phase verification and review results live in `docs/plans/RUST_BITGET.md`.

## Account and close diagnostics

`bitget.account.mode` is an Info engine event emitted once after the first loaded
private account snapshot, including frame-fed startup. It reports `accountMode`
(`classic`/`uta`), `posMode` (`one_way`/`hedge`, or `unknown` without evidence),
and the known `marginMode` (`crossed`, `isolated`, `mixed`, or `unknown`). Public-only
engines do not emit it.

`bitget.close.diagnostic` is a Warn engine event for a closing request's native
failure or a different first native booking size. It carries symbol/position
side, the exact native `requestedSize`/`submittedSize`, the store's
`positionTotal`, and counts/sums of remaining open regular reduce-only orders
and plan orders for that symbol/side. The counts exclude terminal and opposite
side rows; shared plan identities count once. Inverse order/request quantities
are native contract units; `positionTotal` remains the store's base quantity.
The event never rewrites an order, classifies a command, or authorizes a retry.

`nativeCode` and `nativeMsg` come from the original top-level response or each
`failureList` row, independently of classification and user-facing text. Numeric
code spelling and string leading zeros are retained; missing values are empty
strings with their `nativeCodePresent`/`nativeMsgPresent` flags false, never an
inferred 40757. A booking observation without a native error uses those absent
flags. Existing engine string limits still apply (512 UTF-8 bytes per field).
A batch failure is matched by its original client identity (or an unambiguous
single-row request). A proven opening-row failure emits no close diagnostic.
Missing, foreign or conflicting identities in a multi-row batch emit one event
with `requestAttribution=ambiguous` and the raw code/message, without guessing a
symbol, side, size or reservation count. Exact matches and booking observations
carry `requestAttribution=exact`.

Booking correlation retains at most 512 closing requests for five minutes and
consumes each on its first matching native observation. Older REST reads cannot
answer a newer request. Matching bookings emit nothing; repeated stream/read
frames cannot emit another mismatch. The observation is diagnostic only.

The Rust adapter serializes exactly these two events in the ordinary browser log
string, so the worker bridge and terminal logging retain every field. The native
host journals their JSON-escaped event notices once, including startup without a
browser peer; other event logging retains its existing behavior. No new Sentry
tags are introduced. Venue-private available/locked receipt binding follows PR A's
landing; this initial telemetry change does not guess those values from total.
