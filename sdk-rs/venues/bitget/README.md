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

Classic and UTA normal orders use the legacy TS allocation: round the requested amount,
repeat the rounded equal lot `ceil(amount / max)` times, then append the rounded
remainder. Per-lot rounding can change the submitted total. Batch chunks retain
their 50-row limit and the existing partial-failure reporting.

UTA fills use the dedicated execution channel. UTA account topics cover multiple
products, so orders are filtered by category and positions by margin coin and
known market. Combined standalone TP/SL strategies use separate leg identities
which resolve to their shared native strategy ID.

- [UTA fill channel](https://www.bitget.com/legacy-docs/uta/websocket/private/Fill-Channel)
- [UTA order channel](https://www.bitget.com/legacy-docs/uta/websocket/private/Order-Channel)
- [UTA strategy orders](https://www.bitget.com/legacy-docs/uta/websocket/private/Strategy-Order-Channel)

Trading uses sequenced jobs for orders, brackets, conditional stops and account
settings. Native amendments send a fresh `newClientOid`; cancellation retains
the original native `loss_plan` or `pos_loss` identity. Ambiguous writes halt the remaining writes of that action and are
never retried. This is the recorded stop-after-UNKNOWN parity exception
(Sean/daxx, 2026-11-08): legacy TS can continue after an unproven acknowledgement.
Venue-authored terminal write failures retain acknowledged IDs and name the
unconfirmed request with
`The outcome is unknown. Check Open Orders`; the companion
`order.outcome_unknown` event reports the command, acknowledged count and
unconfirmed request. Engine-sealed failures retain the engine's canonical text. History reads page executions, orders, income and public trades
into the engine query shapes and reject incomplete financial results.

After a placement refusal, dispatch follows the legacy TS path: Classic
`failureList` rows and HTTP failures stop later batches and standalone TP/SL;
UTA normal and strategy refusals stop subsequent writes. Classic batch HTTP-200
business refusals without a `failureList` continue to later batches and TP/SL,
while Rust retains the refusal and acknowledged partial identities in its terminal
error (`kind=rejected`, with no outcome after partial success). Legacy TS resolves
with the collected IDs instead. This terminal-classification parity exception is
pending Sam's decision; the current pins deliberately retain the Rust result.
Native amend rejection still releases its cancel/replace fallback. A rejected
UTA protective replacement still attempts the original stop once; a refused
restore ends the sequence. Zero-rounded anonymous rows remain omitted before
dispatch. These rules add no whole-plan sizing gate or tail rebalancing.

Classic entry presets expose no documented trigger selector; their default is
UNPROVEN without a DEMO capture. Explicit selections refuse before send, and
preset children retain only their returned `fill_price` / `mark_price` reference.
UTA entries and Classic/UTA position TP/SL honor Last and Mark; Index refuses.
See [trigger choices and UI integration](../../../../docs/plans/callers/bitget-tpsl-trigger-types.md).

Inverse sizing and UTA bills/trailing remain outside the verified scope.
Phase verification and review results live in `docs/plans/RUST_BITGET.md`.

Classic TPSL rows explicitly marked `orderType=market` with an absent or blank
`executePrice` retain market execution in REST, WebSocket and version-2 source
frames. This includes the documented `psl`, `ptp`, `sl` and `tp`
[WebSocket plan aliases](https://bitgetlimited.github.io/apidoc/en/mix/#websocket-plantype),
normalized to their position-level or sized TPSL kind. Native TPSL amendments
send `executePrice="0"`, the documented market
form, while preserving the order identity, quantity and trigger reference.
Unknown or malformed execution evidence stays refused; it never borrows a live
price. Version-1 frames still lack this evidence and require re-placing the stop.
See [Classic modify TPSL](https://www.bitget.com/docs/catalog/classic-contract-plan/classic-contract-plan).

## Account and close diagnostics

`bitget.account.mode` is an Info engine event emitted once after the first loaded
private account snapshot, including frame-fed startup. It reports `accountMode`
(`classic`/`uta`), `posMode` (`one_way`/`hedge`, or `unknown` without evidence),
and the known `marginMode` (`crossed`, `isolated`, `mixed`, or `unknown`). Public-only
engines do not emit it.

`bitget.close.diagnostic` is a Warn engine event for a closing request's native
failure or a different first native booking size. It carries symbol/position
side, native-lot-formatted `requestedSize`/`submittedSize`, the store's
`positionTotal`, and counts/sums of remaining open regular reduce-only orders
and plan orders for that symbol/side. The counts exclude terminal and opposite
side rows; shared plan identities count once. Inverse order/request quantities
are native contract units; `positionTotal` remains the store's base quantity.
Size fields are decimal strings normalized to native lot precision; counts stay
numeric. Booking comparisons use the same normalization, so binary floating-point
round-trip noise does not report a mismatch; a one-lot difference does. Inverse
base `positionTotal` is formatted without applying the native-contract lot.
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
carry `requestAttribution=exact`. A native failure-list row has reason `rejected`
even when its request attribution is ambiguous; this is a diagnostic label,
independent of the command's unchanged native-code outcome classification.

Booking correlation retains at most 512 closing requests for five minutes and
consumes each on its first matching native observation. Older REST reads cannot
answer a newer request. Matching bookings emit nothing; repeated stream/read
frames cannot emit another mismatch. The observation is diagnostic only.

The Rust adapter serializes exactly these two events in the ordinary browser log
string, so the worker bridge and terminal logging retain every field. The native
host journals their JSON-escaped event notices once, including startup without a
browser peer; other event logging retains its existing behavior. No new Sentry
tags are introduced.

Exact-attribution close diagnostics also include `positionAvailable` and
`positionLocked` as native-lot-formatted decimal strings when `admitted_split`
returns a receipt for that symbol/side. This accessor certifies the exact
admitted row occurrence, unique Store row, update time, contracts and current
position epoch. A missing, retired, stale, ambiguous or otherwise unbound receipt
omits both fields; they are never inferred from position total. Ambiguous batch
failures carry neither field. Frame-fed consumers without their own native split
receipt also omit them; frame/model quantities never stand in for a receipt.
The same read-only binding applies to refusals and first-booking size mismatches
and adds no event or request.
