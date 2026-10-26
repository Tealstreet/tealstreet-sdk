// TS oracle fixers have no access to candidate outputs. This module receives
// only the reference output and the input tape; keep it isolated from gate.mjs.
import { parseRequestJson } from './request-json.mjs';
import { fixBlofinAccount, fixWeexConditionals, fixPhemexAccount, fixLighterFills, fixLighterCommands, fixKrakenOutput } from './known-ts-fixes.mjs';
import { fixCoinbase } from './known-ts-fixes-coinbase.mjs';
import { fixOrderly } from './known-ts-fixes-orderly.mjs';
import { fixBitrue } from './known-ts-fixes-bitrue.mjs';
import { fixBinanceSpotFills, fixBinanceFilledStatus } from './binance-protocol.mjs';
import { fixPositionMargins } from './margin-expectation.mjs';
import { venueOf, ORDERLY_BROKERS } from './venues.mjs';
import { normalizeTsBybitSubscriptions } from './bybit-subscriptions.mjs';
import { withMexcMiniTickerSubscription } from './mexc-mini-subscription.mjs';
import { expectedKucoinRequests } from './kucoin-request-contracts.mjs';
import { requestKey, subscriptionKeys, callerIdsOf } from './requests.mjs';
import { applyLegacyFillOracle } from './legacy-fill-oracle.mjs';

export function createTsFixes(values, tapeRecords) {
const records = tapeRecords;
const instrumentRows = tapeRecords
  .filter((r) => r.type === 'http' && new URL(r.url).pathname === '/v5/market/instruments-info')
  .flatMap((r) => JSON.parse(r.body).result?.list ?? []);
const instruments = new Map(instrumentRows.map((r) => [r.symbol, r]));


  const venue = venueOf(values.venue);
const KNOWN_TS_FIXES = {
  // KuCoin Spot: emit each native execution once even across replay, preserve
  // newer private state across stale REST snapshots, and retain good orders
  // when active-symbol discovery fails or active-order rows have invalid
  // financial geometry. Reject malformed WS order/execution quantities and
  // missing/null required REST balances rather than committing coerced zeros.
  // Encode caller order IDs as URL components on cancel.
  // Native regressions cover these cases;
  // standard replay compares both outputs without rewriting either result.
  kucoinspot() {},
  hyperliquidspot: (ts) => {
    // Paired reference fixes retain native spot IDs for coinless contexts and
    // return string IDs for both resting and filled placement ACKs. Native
    // tests additionally pin spot-only safety paths without rewriting economics.
    // Notes:
    // Join coinless spot contexts positionally only when the entire context array is coinless and exactly matches universe length.
    // Resolve both metadata token indices so native pairs with a shared base retain distinct quote symbols and action/book identities.
    // Convert non-USD quote prices before wallet valuation, prefer direct USDC and reject canonical-token cycles while retaining unpriced raw holdings.
    // Use the spot native websocket endpoint and supported book precision subscriptions (null and 2 through 5).
    // Return the SDK string-array contract for resting and immediately filled orders.
    // Keep execution tid as the TS fill id and expose native oid as orderId for chaser tracking and native fill parity.
    // Subscribe to documented spotState balance updates and remove ignored feeds and perpetual-only activeAssetData subscriptions.
    // Keep absent spot oracle/funding fields finite instead of emitting NaN.
    // Use spot asset indices and 8-minus-size-decimals price precision with no perp builder or reduce-only flags.
    // Keep accepted orders available for immediate cancel/amend despite the TS spot fetchOrders stub.
    // Apply live full/partial executions before a delayed resting acknowledgement without resurrecting the filled native order.
    // Reject unsupported spot leverage, margin transitions and standalone trigger placement before a write.
    // Deduplicate native live executions and suppress historical fill snapshots.
    // Retry failed bootstrap reads without replacing a valid market table with an empty snapshot.
    // The TS spot fetchOrders stub never reads open orders, so external cancels
    // and pre-existing orders never reconcile; Rust reads frontendOpenOrders.
    // Expect exactly the reads the tape declares; Rust requests are never read.
    const callerIds = callerIdsOf(tapeRecords);
    const expected = new Set(ts.requests);
    for (const record of tapeRecords) {
      if (record.type !== 'http' || record.method !== 'POST' || typeof record.requestBody !== 'string'
        || new URL(record.url).pathname !== '/info') continue;
      if (parseRequestJson(record.requestBody)?.type !== 'frontendOpenOrders') continue;
      expected.add(requestKey(record.method, record.url, record.requestBody, callerIds, venue));
    }
    ts.requests = [...expected].sort();
  },
  krakenspot: {
    // Public fixes are paired in TS; independent native tests cover stricter
    // execution, retry and amendment paths. Replay output stays unmodified.
    notes: [
      'Translate AssetPairs XBT and XDG/XXDG identities to accepted WS v2 BTC/DOGE wire symbols while preserving canonical store symbols.',
      'Value normalized BTC holdings through the market identity even when the SDK ticker uses XBT/USD.',
      'Keep native wallet quantities and prior USD while supported conversion prices or rejected catalogue peers recover; only a clean catalogue proves an asset outside the supported aggregate scope.',
      'Parse documented decimal tick and lot steps canonically instead of exposing Math.pow ULP artifacts as extra precision.',
      'Trim subscribed book depth after each update and clear reconnect book caches.',
      'Fan supported native OHLC updates out to every requested fallback timeframe.',
      'Read current native triggers.actual_price/static triggers.price and map BTC/USD execution identity back to XBT/USD.',
      'Keep documented iceberg orders as limit orders with native amend provenance.',
      'Derive trailing quote distances from the raw chart input before precision rounding.',
      'Sign exact form bytes and enforce monotonic nonce, including retries and reconnect token reads.',
      'Refresh the authenticated WS token for each socket generation.',
      'Emit exact per-execution last_qty/last_price once per native execution identity.',
      'Treat filled lifecycle rows as status transitions rather than extra executions, including trade/filled rows batched in either order.',
      'Preserve native order txid across atomic amendments and reject ambiguous edits before a write.',
    ],
  },
  // TxFlow previously rejected start as a scaffold. Its new TS reference and
  // Rust implementation both fence stale reads, deduplicate executions, reject
  // uncertain write acknowledgements and revalidate queued amendments.
  // Preserve sparse-context volumes, authoritative hedge settings/mode fences,
  // and delisted history identities in both implementations as well.
  // Compare both outputs directly; no TxFlow-specific normalization is permitted.
  // The production wrapper also rejects order-history ID/status filters that
  // the native query cannot represent, instead of silently discarding them.
  // Both implementations pace subscription bursts and recover dropped gateway
  // subscriptions; the live fixture confirms every ticker acknowledgement.
  txflow: [],
  breakoutprop: {
    // Public polling, duplicate quote levels and the cancellation precondition
    // are fixed in the TS reference. Native tests pin safety corrections; no
    // account economics or request values are replaced by this entry.
    notes: [
      'Select exact account code/suffix instead of a substring that can trade another account.',
      'Fence concurrent old-session 401s behind the newer login generation.',
      'Retain native positionCode so same-symbol lots are not silently collapsed by DefaultStore.',
      'Validate selected portfolio identity and publish sections atomically after parsing.',
      'Read documented top-level limitPrice/stopPrice while retaining legacy leg-price payloads.',
      'Fence stale portfolio reads behind acknowledged writes.',
      'Public-only sessions poll quotes; synthetic books contain one placeholder level per side.',
      'Placement returns the SDK string-array order ID contract rather than a raw native number.',
      'Send the standard If-Match precondition on cancellation; retain ACK identity for immediate cancellation.',
      'Reject silently ignored attached protections, unsupported trigger sources, close flags and maxSlippagePct before any batch write.',
      'Reject unsafe amendments after partial fills or competing order changes.',
    ],
  },
  // Paired TS fixes: retain both rolling-volume sides and open price across
  // ticker deltas; derive quote volume/change from current last; accept native
  // empty-string clears and nanosecond funding timestamps in either JSON form.
  // Rust also anchors sequences from subscribe ACKs and isolates candle intervals.
  // Private reads preserve zero free collateral, exact native IDs/timestamps and
  // CROSS margin metadata; failed/malformed/stale reads never clear live state.
  // Successful revision-matching empty snapshots remove ghosts and retain
  // tombstones against delayed stream events. Session code 1000 refreshes once
  // even when delivered in an HTTP-200 error envelope.
  // Orders use exact EIP-712 integers, distinct reducing bracket children,
  // exact-instrument cancellation and remaining quantity on partial-fill amend.
  // Conditional replacement retains native execution price separately from
  // its trigger; mapped orders keep explicit client IDs. Refreshed cookies
  // agree with proxy-session headers and reconnect the private socket.
  // The private server ping receives pong rather than being ignored.
  // Rust also refuses replacement after observed execution/lifecycle progress
  // or stale cancellation proof, rechecking deferred/resigned dispatches.
  // Mixed products retain native kind/hash; full raw open-order validation
  // precedes individual symbol cancellations and preserves sibling contracts.
  // Raw ACK checks stop rejected/ambiguous writes; client-only cancellations
  // cannot silently reauthenticate and retarget a reused client identity.
  grvt() {},
  polymarket() {
    // Missing native holding prices cannot use a BBO-only default last=0 or
    // entry price as observed valuation; unavailable owned metadata fails visibly.
    // Reference repairs (not output normalization): preserve initial array books,
    // every sorted absolute depth change, dynamic ticks and event watermarks;
    // use complete Gamma keyset discovery and interval-minute sampled candles.
    // Native regressions additionally pin: one fill per owned execution across
    // settlement statuses, zero resolution price, failed/stale snapshot guards,
    // reported partial batch IDs, partially matched GTC acknowledgements, and
    // coherent final-fill cancel proofs with replacement leaves rounded down.
    // Share sizing uses financial half-up precision, correcting TS toFixed's
    // binary midpoint artifact; JavaScript formatting is never emulated.
    // Unbounded history supplies the API max selector; upper-only and zero-wire
    // start timestamps reject explicitly. Paired cumulative/execution evidence
    // and association-covered final reads fence amend races and missing fields.
    // Authoritative order removals cannot be reopened by delayed stream frames.
    // Unsupported history contracts fail explicitly instead of empty success.
  },
  // Independent documented payload oracle; paired public spot reference repairs
  // leave the real TS Store/worker serialization and measured CPU/memory intact.
  // Also known TS defects: a batch_cancel result with success:false still marks
  // the order canceled, and a paced census can apply a pre-cancel page with no
  // terminal fence. Checkpoints compare raw output, so those paths stay declared.
  coinbase: (ts) => fixCoinbase(ts, tapeRecords),
  coinbasespot: (ts) => fixCoinbase(ts, tapeRecords),
  phemex(ts) {
    // Paired TS/Rust REST balance totals now exclude position UPnL just as
    // their WS handlers do; native free remains reported total minus used.
    // Authority fences protect reads across pushes, socket generations and
    // other REST commits; compare balances without financial normalization.
    // Independent execution/cumulative-size/flat-settings oracle from taped venue rows.
    if (ts.account) fixPhemexAccount(ts.account, tapeRecords);
    // Phemex zero-open ticker changes remain finite.
    for (const ticker of ts.tickers) if (ticker.percentage === null) ticker.percentage = 0;
    // TS recovery reads missing/unknown affected sides and preserves nonzero
    // full-snapshot rows while recovering the other hedged leg.
    // Native tests also guard exact row versions/absence fences, confirmed
    // cancellation finality, preserved trigger sources and exact split lots.
  },
  // Paired production fixes, compared without rewriting either replay result:
  // preserve full-depth incremental books; verify subscription acknowledgements;
  // fence polled candles against newer trades and reject older trade buckets;
  // apply every per-market BBO/summary update; emit unique private executions;
  // clear closed positions and classify CLOSED orders without reopening them;
  // negotiate/decode mandatory SBE 1:2; use RFC6455 rather than invalid JSON
  // heartbeats and accept the documented auth node identity; preserve
  // venue JWT Authorization beside Tealstreet-Authorization on routed requests;
  // preserve native client IDs and canonical time-in-force; derive account UPnL
  // from positions; support idempotent cross-margin/one-way mode settings;
  // validate public-trade query windows and deduplicate native fill IDs/quantity.
  // Walk empty history cursor pages and keep polled candle buckets/volume monotonic.
  // Rust also fences lagging order-repair rows with native timestamps/exact fill
  // quanta and retains exact accepted/ambiguous amendment targets after ACKs;
  // adversarial native tests cover this without rewriting TS replay output.
  // Security audit: Rust auth signatures expire after 300 seconds; TS keeps
  // 86400. The protocol audit binds the lifetime separately per implementation
  // and verifies both signed preimages; no output normalization is needed.
  // Rust preserves accepted IDs from partial native/sequential placement batches,
  // continues independent orders after rejection, surfaces failed protection at
  // Error level, retains structured rejection reasons and sends integer leverage.
  // These TS defects have native regressions; no replay normalization is applied.
  paradex() {},
  // ApeX repairs are in the TS reference; output is compared without rewriting:
  // continuous book sequences and atomic invalid depth updates; top-level ping
  // detection avoids answering echoed ping acknowledgements; stable original
  // payloads for legacy HMAC across form transforms; canonical hedged position
  // IDs; failure-safe private snapshots and required row identifiers;
  // documented realizedPnl aliases; raw-millisecond history bounds/order;
  // one fill per genuine execution, isolated synthetic replay identity;
  // distinct equal executions preserved in REST/WS history; exact tick splitting;
  // both bracket siblings preserved during parent cancel/replace amendments.
  // Price-only amendments keep the observed residual quantity; invalid/no-op
  // amendments reject before cancellation and filled sources are not replaced.
  // Balance UPnL uses the live last price like TS; with only a zero/invalid
  // placeholder quote Rust uses reported position PnL (TS prices it at 0).
  // TS logs native login rejection and has no auth-completion deadline.
  // Native failure regressions verify closure/backoff without altering replay output.
  apexomni: [
    'Close rejected private logins and bound missing login ACK/account-stream readiness independently of pongs; preserve immediate subscriptions.',
    // Private tape: real quote 60000 == entry 60000 and native unrealisedPnl 0,
    // yet TS shows balance 1300 / upnl -300 at startup checkpoints 0-2.
    'Never price positions at an all-zero placeholder ticker: TS fabricates a startup loss (balance total/upnl) until its next balance fetch.',
  ],
  // TS only reports public/auth errors; heartbeat responses can mask missing auth.
  cryptocom: ['Close rejected private authentication and enforce an independent auth deadline while preserving server heartbeat replies.'],
  kraken: {
    output: fixKrakenOutput,
    fixes: [
      'Retain full depth, reject sequence gaps/stale snapshots, unsubscribe unused books.',
      'Validate Kraken Spot top-ten CRC32 using original decimal tokens; TS ignores checksum.',
      'Full native ticker statistics stay live; omitted relative funding resets zero instead of retaining a stale rate.',
      'Negative instrument quantity precision is a positive power-of-ten step; TS clamps it to 0.1.',
      'Deduplicate native execution IDs; suppress historical fills snapshots.',
      'Use monotonic nonces beyond 1000 same-millisecond requests and clock regression.',
      'Use documented index triggerSignal (TS sends stale spot enum).',
      'Keep accepted settings and newer socket state across delayed reads/write ACKs.',
      'Replace partially executed protective orders with remaining size and stop on raced fills.',
      'Reject missing/contradictory semantic ACKs and failed/incomplete reads explicitly.',
      'Keep exact native milliseconds for fill filters/cursors before seconds output.',
      'Count raw ledger rows for pagination rather than mapped cash components.',
      'One-way mode is an accepted no-op; TS inherits an unimplemented setter.',
      'Use valid authenticated feeds and REST balance instead of invalid account_balances.',
      'Preserve absent position margin allocation instead of TS base-store synthetic zero.',
      'Map native post/ioc/fok orders as limits and reject unsupported opening conditional shapes before a write.',
      'Keep initialized event-driven private feeds alive while bounding retained execution and terminal identities.',
    ],
  },
  asterdex: {
    // Native payload tests exercise these corrections; ordinary tapes compare
    // unchanged, including exact execution prices and economic request fields.
    notes: [
      'A zero-size hedge WS update closes only its reported leg; TS removes every position for the symbol.',
      'Nonce uniqueness is scoped to the agent address as documented; TS scopes it to owner plus agent and can collide across owners.',
      'Failed or malformed history reads propagate errors instead of silently returning an authoritative empty result.',
      'Historical candle timestamps use engine seconds consistently; TS returns raw milliseconds on this REST path.',
      'Do not submit an amendment replacement after the old order fills during cancellation, including fills reported by the cancel response; an unknown-order rejection cannot authorize replacement.',
      'Configured leverage reads use the actual native setting rather than the inherited TS market-maximum fallback that overwrites it.',
    ],
  },
  hyperliquid() {
    // Native command regressions pin these deliberate TS fixes:
    // - Validate every cancel acknowledgement before cancel+replace continues.
    // - Resolve queued leverage/margin settings when the step executes.
    // - Keep each HIP-3 bracket leg's own metadata in pending/optimistic state;
    //   TS assigns entry metadata to every returned oid in placeLimitOrder.
    // - History keeps distinct tid executions and signed rebates, uses vault
    //   identity and caller ranges, and rejects failed or saturated reads.
    // - Execution limits sort raw milliseconds before serializing seconds.
    // - Income honors the requested kind; fills support all-symbol queries.
    // - Defer unified/portfolio collateral valuation until real FX and a core
    //   exposure baseline exist; retain pending native balances across delayed
    //   REST and recover on live/shared/discovery prices without fake zero USD.
    // The tape compares converged state: no account values are rewritten here.
  },

  // Independent documented-tape oracle; native request/protocol counters remain exact.
  orderly: (ts) => fixOrderly(ts, tapeRecords),
  lighter: {
    fills: fixLighterFills,
    commands: fixLighterCommands,
    // Native tests pin these defects without rewriting account or trading output.
    notes: [
      'Emit one execution by native trade ID; TS emits both the filled order and the execution.',
      'Keep valid live executions during the initial five-second notification guard and inherit by-market execution identity.',
      'Value account positions from native unrealized_pnl and position_value before public prices; missing PnL retains prior same-side valuation instead of inventing a zero-price entry loss.',
      'Read execution history from trades, not cumulative inactive orders; preserve exact IDs and timestamp range units.',
      'Convert native venue/integrator fee rate ticks to signed quote fees and rebates; retain venue-reported account-side PnL.',
      'Validate integrator approval expiry and both runtime fee caps before attributed order submission.',
      'Keep known orders after definite cancel rejections, and normalize official hyphenated terminal statuses and time-in-force.',
      'Use engine socket watchdogs rather than a lifetime close-loop cap.',
      'Preserve account and open orders after failed, malformed, or capped reads.',
      'Fence delayed REST snapshots, fills, and acknowledgements behind newer private state.',
      'Require a nonempty transaction hash for every batch item; TS accepts missing batch acknowledgements.',
      'Reject book nonce gaps and pre-gap REST recovery snapshots.',
      'Reject API key index 255, the query-all sentinel, for signing.',
      'Support the full official int16 market domain; TS leverage still rejects active IDs above 254.',
      'Acknowledge retaining the only supported net position mode; TS inherits an unsupported method.',
      'Price-only cancel/replace amendments re-place the remaining quantity after partial execution; TS re-places the original amount and increases exposure.',
    ],
  },
  // WOO repairs live in the TS reference itself; no result normalization:
  // exact integer IDs; order-axis journal provenance; camel-case aliases;
  // monotonic cumulative/terminal state and reconciled optimistic fill deltas;
  // nested algo child executions and minimal terminal parent tombstones;
  // execution-ID dedup and per-execution fills; atomic failed-page snapshots
  // including malformed nested executable leaves;
  // STOP direction/source and buy-trailing inference; exact tick splitting;
  // sized protections, full-position trailing and sibling preservation;
  // mandatory rounded amendment quantities; both-family cancellation;
  // unsupported Conditional preflight; authoritative account hedge mode;
  // clear synthetic flat marginSize when an authoritative holding reopens;
  // paced, complete V1 income with signed fees and account aggregate scope.
  // Rust additionally walks V3 fill time windows (TS still reads one page).
  // TS only emits an error for auth.success=false; transport pongs are not auth.
  woo: [
    'Validate prevTs continuity and clear resync books; TS accepts unbridged depth deltas.',
    'Close rejected private authentication and bound pending authentication independently of pongs.',
  ],
  // Paired TS fixes: contract-sized public volumes; private cumulative z/native
  // units, execution L prices, monotonic terminal state and ping timestamp echo.
  // Cancel native identities and protective defaults now match metadata. Rust
  // also conserves split quantities, rejects unknown settings, and verifies
  // final cancellation/protection provenance before placing replacements.
  // Fill history no longer borrows counterparty IDs or invents an order ID;
  // filtered full pages fail INCOMPLETE and rejected reads propagate errors.
  // Conditional REST children retain exact IDs/parents and trigger references.
  // Fresh TS private rows now retain native/cached TIF and default to GTC,
  // matching REST and client-ID lookup without normalizing command results.
  // Rust rejects malformed candle timestamps instead of returning partial history.
  bitrue(ts) {
    // Rust refuses zero/non-finite ticker-derived position losses; per-currency
    // UPnL is sumOpenRealizedAmount exactly as TS publishes it. Confirmed
    // leverage survives flat/empty account metadata; TS clears that rail and
    // blocks fresh-account orders. Native regression tests cover this
    // correction; no account values or command results are normalized here.
    // Flat rows may omit leverage until explicitly set; balance still commits.
    // Cancellation continues after failures, including child fallbacks after a
    // failed parent, and reports each failed ID. TS stops subsequent chunks.
    // TS assigns a cumulative gap to the last native L and drops an older l/L
    // execution. Only the exact private fixture and exact legacy TS fills may be
    // corrected; the independent l/L table never reads or changes Rust output.
    fixBitrue(ts, tapeRecords);
  },
  bitget: {
    // Each correction is covered by documented-payload Rust tests. Normal
    // tapes still compare unchanged; these notes do not rewrite mismatches.
    notes: [
      // TS only logs event=login failures; authenticated liveness needs its own deadline.
      'Close rejected private login and enforce an auth deadline independent of transport pongs.',
      'Reject book sequence gaps and legacy checksum failures.',
      'Preserve account state on failed business-error reads.',
      'Filter all-product UTA streams to this futures product.',
      'Use UTA execution IDs, quantities and prices from the fill topic.',
      'Keep both TP/SL legs of status-less UTA strategy rows.',
      'Include UTA totalFundingFee in realized PnL.',
      'Derive UTA TPSL closing direction from the protected position side, preserving ordinary execution directions.',
      'Use cumulative executed size for orders and deduplicate executions.',
      'Do not emit an execution for an algo trigger acknowledgement.',
      'Split normal orders at floored max lots without losing requested volume.',
      'Send native crossed marginMode for Classic batches rather than cross.',
      'Resolve native modify identity using the fresh newClientOid.',
      'Never cancel-replace an amendment after an ambiguous transport/server result.',
      'Report incomplete or malformed financial history as a failed read.',
      'Read UTA bills through financial-records; retain raw signed deltas and fee metadata without assuming gross PnL.',
      'Support documented UTA trailing strategies without unsupported clientOid fields.',
      'Amend Classic trailing via modify-plan-order and cancel with orderIdList; preserve callback on size-only edits.',
      'Use documented Classic hedge-close trailing side and validate callback/lot before writes.',
      'Normalize inverse private WS sizing and aggregate all collateral coins in USD like REST.',
      'Clamp inverse sized stops in native contracts and preserve size on amend instead of downgrading to full-position.',
    ],
  },

  blofin: (ts) => {
    // TS recognizes successful login only; rejection/pongs can leave auth pending.
    // Rust closes/backoffs rejected login and enforces an independent deadline.
    // Native failure tests prove this; no replay liveness output is rewritten.
    if (ts.account) fixBlofinAccount(ts.account);
  },
  mexc: (ts) => {
    // The official sub.tickers sparse all-contract feed prevents stale prices
    // past the first 100 detailed subscriptions. Add that exact expected TS
    // subscription; prices and Rust output remain independently compared.
    ts.subscriptions = withMexcMiniTickerSubscription(ts.subscriptions);
    // A singleton skips TS placeOrders' market-maker batch probe; its exact
    // rejected TS-only attempt is declared in the tape, not normalized away.
    // TS only logs rejected rs.login; Rust closes/backoffs and bounds auth
    // independently of rs.ping. Native regressions cover the failure path.
    // Full incremental depth, rather than replacing a 20-level snapshot.
    // Sized stops choose the closing leverage rail even without reduceOnly.
    // Bracket prices reach the native entry; unsupported opening triggers reject.
    // Cancel-all reaches regular, plan and position-stop endpoints.
    // Cancel/replace recomputes remaining size after racing fills; Closed wins
    // over a late cancellation acknowledgement. Failed reads preserve state.
    // Regular cumulative fills stay monotonic; terminal rows never reopen.
    // Stale amend reads cannot regress those fills or place a replacement.
    // REST/optimistic rows retain cumulative fills; amendments retain attached
    // brackets and standalone stop sources through venue reconciliation.
    // Native bracket clears and corrected sources win over delayed placement acks.
    // Repeated private execution IDs emit one fill within the replay window.
  },
  kucoin: [
    'futures inherits empty fills/orders/income and unsupported public trades; query results use explicit documented tape oracles',
    'stops classify with side and direction, retain trigger basis and closeOrder',
    'match executions deduplicate trade IDs and require their execution price',
    'failed and delayed REST snapshots preserve newer account state and terminal orders',
    'conditional orders and mixed bracket trigger references fail before writes instead of dropping fields',
    'explicit protected-market slippage is rejected before batch I/O rather than silently dropped',
    'Bybit-only BBO and tick-slippage controls are rejected on KuCoin before batch I/O',
    'open orders walk every numbered page; TS reads only its default first page',
    // TS cancelNormalOrders throws on a rejected ID before attempting later IDs.
    // Native mixed-batch regressions prove continuation and a visible final error.
    'one rejected cancellation does not prevent later IDs from being attempted',
    'socket readiness and subscriptions wait for welcome; TS sends on TCP open',
    'cancel-replace proves terminal cancellation and subtracts fills during cancellation',
    'stop-to-normal migration remains visible across independent REST books',
    'settings streams update flat symbols and fence delayed metadata reads',
    'position mode uses its authoritative read rather than inferring it from positions',
    'independent tape contracts require native query/cancel-proof/mode reads and single-page active-book aliases',
    'partial order stream rows retain REST-known safety flags',
    'immediate bracket replacement retains submitted protections before reconciliation',
    'cached book snapshot gaps retain the entire contiguous buffered suffix (TS reference fixed with a regression)',
  ],
  // PM TS cancelSymbolOrders deletes regular orders only, leaving conditional
  // stops live. Rust's independent fixture plan requires both DELETE families.
  binancepm: (ts) => KNOWN_TS_FIXES.binance(ts),
  // Spot aggregate !ticker@arr/!bookTicker streams were retired; the TS replay
  // adapter sends documented per-symbol ticker shards and uses TS's own handlers.
  // Signature-only subscriptions use session.subscriptions, not session.status.
  // COIN-M's documented fixes also have native regression proofs: exact split
  // totals, plain-trigger/caller-ID whole-batch rejection, post-only amendment,
  // cumulative-fill/terminal replay fences and malformed openOrders retention.
  // TS cancel-all marks already FILLED rows canceled; use explicit tape events.
  // Trading audit: derivatives require x=TRADE, deduplicate native trade IDs,
  // and report genuine late trades after stop-trigger EXPIRED/cancellation.
  // Native regressions cover those adverse packets; no fill is normalized here.
  binanceinverse: (ts) => fixBinanceFilledStatus(ts.account, tapeRecords, false),
  // Other documented Binance variant fixes have native regression proofs:
  // - TS split lots divide the whole request and can round away quantity;
  //   Rust sends max-size lots plus the exact native-unit remainder.
  // - Spot attached stops inherit entry timeInForce and may exceed max quantity;
  //   Rust removes incompatible fields and splits every protective leg.
  // - Spot drops reduceOnly/close and plain-order triggerPrice; COIN-M also
  //   drops plain-order triggerPrice. Rust rejects before any batch write.
  // - COIN-M overrides silently drop caller IDs despite the base unsupported
  //   contract; Rust rejects the whole batch before placing an earlier order.
  // - TS cancel/replace loses post-only TIF; Rust retains the native semantics.
  // - Spot cancel executionReport.C identifies the original client order; TS
  //   instead targets c, which can be the cancellation request's client ID.
  // - Spot/COIN-M duplicate cumulative executions can emit fills again and
  //   stale NEW events reopen terminal orders; Rust fences by native order ID.
  //   Spot also deduplicates the latest 16,384 native (symbol, order, trade) IDs,
  //   independently of ACK totals and terminal lifecycle (native/SDK proofs).
  // - COIN-M malformed openOrders responses become authoritative empty in TS;
  //   Rust preserves the previous order state and retries reconciliation.
  // - COIN-M TS values an unpriced collateral coin at USD1; Rust keeps its
  //   native amount with USD0 until a real mark/last price is available.
  // - Tiny/negative inverse quantities can round to zero and become full
  //   position stops in TS; Rust requires zero input or explicit close intent.
  // - Spot/COIN-M cancel-replace reuses original total quantity after fills,
  //   accepts unproven cancel bodies and can overwrite Filled with Canceled.
  //   Rust verifies native acknowledgements, then recomputes remaining size;
  //   rejected/ambiguous cancels and filled orders never place a replacement.
  // TS reports resting executionReport.p instead of this trade's L.
  binancespot: (ts) => {
    fixBinanceSpotFills(ts.account, tapeRecords);
    fixBinanceFilledStatus(ts.account, tapeRecords, true);
  },
  bingx: (ts) => {
    const account = ts.account;
    if (!account) return;
    if (values.venue === 'bingx') {
      // BingX TS emits a terminal cumulative fill, ignores partial executions,
      // and drops its terminal watermark so TRADE_UPDATE can repeat that fill.
      // Use the taped per-execution oracle; Rust is still compared against it.
      const watermark = new Map();
      const fills = [];
      for (const record of records) {
        if (record.type !== 'ws' || !record.url?.includes('?listenKey=')) continue;
        let event;
        try {
          event = JSON.parse(record.text, (_key, value, context) =>
            typeof value === 'number' &&
            !Number.isSafeInteger(value) &&
            context?.source &&
            /^-?\d+$/.test(context.source)
              ? context.source
              : value,
          );
        } catch {
          continue;
        }
        if (!['ORDER_TRADE_UPDATE', 'TRADE_UPDATE'].includes(event.e)) continue;
        const o = event.o;
        if (!['PARTIALLY_FILLED', 'FILLED'].includes(o.X)) continue;
        const id = String(o.i);
        const cumulative = Number(o.z);
        const previous = watermark.get(id) ?? 0;
        const amount = Math.max(0, cumulative - previous);
        watermark.set(id, Math.max(previous, cumulative));
        if (amount > 0)
          fills.push({
            id,
            symbol: o.s.replaceAll('-', ''),
            side: o.S === 'BUY' ? 'buy' : 'sell',
            amount: Number(o.l) > 0 ? Math.min(amount, Number(o.l)) : amount,
          });
      }
      account.fills = fills;
      // TS's private mapper reads closePosition but misses the native cp alias.
      for (const record of records) {
        if (record.type !== 'ws') continue;
        let event;
        try {
          event = JSON.parse(record.text, (_key, value, context) =>
            typeof value === 'number' &&
            !Number.isSafeInteger(value) &&
            context?.source &&
            /^-?\d+$/.test(context.source)
              ? context.source
              : value,
          );
        } catch {
          continue;
        }
        const row = account.orders.find((o) => o.id === String(event.o?.i));
        if (row && event.o.cp !== undefined) row.close = event.o.cp === true || event.o.cp === 'true';
      }
      return;
    }
  },

  weex(ts) {
    const account = ts.account;
    if (!account) return;
    // TS reads opening STOP/TP stream rows as protective orders and displays
    // the execution price as their trigger, corrupting conditional size amends.

    fixWeexConditionals(account, records);
    // Additional deliberate fixes, covered by native regressions: failed
    // normal/algo reads retain the complete last snapshot; stale depth/private
    // frames cannot undo newer state; repeated fill IDs emit once; missing
    // placement IDs fail rather than fabricating an ID; rejected cancellations
    // retain their order; per-symbol margin changes preserve that symbol's mode.
    // Price-only replacement preserves remaining quantity and refuses uncertain
    // fill/bracket state; positive income cursors always report INCOMPLETE.
  },

  okx: [
    'full books retain levels past a stale last trade',
    'collateral uses proven native or public FX, never a one-dollar fallback for missing nonstable prices; retain native currencies and prior USD until valuation is available, and hand inverse margin, PnL and normalized exposure to the same recovered quote',
    'paired TS/Rust fix: untraded X-Perp blank quote fields seed and stream finite zeros and zero-open percentage stays zero; one unpriced ticker cannot abort the whole catalog',
    'public trades implement the inherited Not implemented method',
    'candle calendar bounds use UTC independently of host timezone',
    'fill window filtering retains native milliseconds before output seconds are truncated',
    'keyless/public-only business socket avoids empty-credential login and private topics',
    // Rust safety fixes: covered independently by documented-payload tests.
    'split bracket entries acquire SL/TP before the next entry is placed',
    'native OCO amend preserves its sibling protective leg',
    'negative protective sizes and market or terminal amends fail before writes',
    'positive protective dust cannot round into a close-entire-position order',
    'standalone protective limit amendment fails before cancel if execution price cannot be retained',
    'standalone cancellation continues remaining batches after per-row failures',
    // Paired TS/Rust fix; account output is still compared without normalization.
    'account totalEq stays separate from real USD currency equity; delta currencies, live-order margin and revision fences survive delayed reads and local ticks',
    'acknowledged leverage survives failed post-write snapshots',
    // Position pushes carry a slice, not whole-account maintenance margin.
    'partial position pushes and account equity updates retain full native IMR, converted PnL and open-order reserve through the next tick',
    // TS finite coercion hides malformed risk; isolated native IMR may be absent.
    'malformed or overflowing native risk cannot become zero; isolated position pushes use reported margin when IMR is unavailable',
    'untraded EEA X-Perp empty last/open seeds initialize to zero and stream updates retain proven trade/percentage instead of NaN',
    // Phase-9: inverse contracts convert at native mark/execution reference;
    // EEA history scopes by native product rather than hardcoded SWAP.
    'EEA historical orders and income query retained FUTURES X-Perps as well as SWAP',
    'composite income limits apply after joining newest whole native bill groups',
    'composite capped or stalled product histories fail explicitly as incomplete',
  ],

  // Trading audit: cancel/replace waits for cancellation evidence, subtracts
  // racing cumulative fills, and refuses unknown-order replacement. Successful
  // writes fence old USD-M/PM snapshots; generated client-ID RNG errors reject.
  // Derivative lifecycle packets cannot emit fills; native trade IDs dedupe
  // retries even after reconnect. Native race tests pin these TS corrections;
  // this entry adds no output or request rewrite for the audit fixes.
  binance: (ts) => {
    // KNOWN_TS_FIXES (security M5): USD-M REST/WS mappers retain native TIF,
    // and cancel-replace copies it so GTX stays post-only. The paired TS fix
    // keeps strict write identity; no TIF or economic output is normalized.
    // Native funded-wallet regressions: absent prices preserve native amounts
    // and prior USD; validated native risk marks can prove startup conversion.
    // Partial metadata cannot prove an asset unsupported, and real public
    // prices supersede startup marks without replacing known live state.
    // KNOWN_TS_FIXES: one-way leverage writes used hedge IDs, leaving the cache
    // stale and skipping restoration; rejected writes were swallowed. Reference
    // setter and deterministic regression are fixed, so no output adjustment.
    fixPositionMargins(ts, tapeRecords, 'binance');
  },
  // Phase-9 native regressions additionally correct TS: account-wide history
  // limits before joining newer inverse/options sources; ignored IV on ordinary
  // derivatives; generic option tickers omitted and option books routed through
  // the linear feed; malformed supplied option WS Greeks coerced to zero
  // instead of retaining proven position/Greek state. Dedicated API tests compare
  // options terms/queries/signing, and assert native option depth25 separately.
  // Trading audit: deduplicate native execId and fence acknowledged Demo/Hyro
  // cancellations behind REST reads issued before, during or after the write,
  // with bounded native terminal retention against stale replicas. Native tests
  // exercise equal-time and late-response races without normalization.
  bybit: (ts) => {
    if (!ts.account) return;
    const output = ts;
    fixPositionMargins(ts, tapeRecords, 'bybit');
    // Optional TS API/VIP UI metadata has no engine-frame representation.
    const rest =
      tapeRecords.find((record) => record.type === 'config')?.options?.endpoints?.rest ??
      (values.venue === 'bybitdemo' ? 'https://api-demo.bybit.com' : 'https://api.bybit.com');
    const metadataRequest = requestKey('GET', `${rest}/v5/user/query-api`, undefined, new Set(), venue);
    output.requests = output.requests.filter((key) => key !== metadataRequest);
    if (values.venue !== 'hyrotrader') {
      const feeds = new Set(output.subscriptions);
      for (const key of output.subscriptions) {
        const match = /^([1-9]\d*) WS (\S+) \{"args":\["order"\],"op":"subscribe"\}$/.exec(key);
        if (!match || new URL(match[2]).pathname !== '/v5/private') continue;
        for (const required of subscriptionKeys(match[2], { op: 'subscribe', args: ['execution', 'wallet'] }))
          feeds.add(`${match[1]} ${required}`);
      }
      output.subscriptions = [...feeds].sort();
    }
    const latest = new Map();
    for (const record of records) {
      if (record.type !== 'ws') continue;
      const event = JSON.parse(record.text);
      if (event.topic === 'order') for (const row of event.data) latest.set(row.orderId, row);
    }
    for (const order of ts.account.orders) {
      if (
        order.rejectReason === null &&
        order.status === 'canceled' &&
        latest.get(order.id)?.rejectReason === 'EC_PostOnlyWillTakeLiquidity'
      )
        order.rejectReason = 'PostOnly';
    }
  },

  // Attached TP/SL must remain market exits. The private tape declares the
  // exact buggy TS request with tsRequestBody; Rust must match requestBody.
  // Native regressions also correct TS repair losses (post-only/TIF/reduce-only)
  // and cancellation races across concurrent writes and REST reconciliation.
  backpack: (ts) => {

    const orders = new Map();
    const positions = new Map();
    const fills = [];
    const executions = new Set();
    const tickers = new Map(ts.tickers.map((t) => [t.symbol, t]));
    for (const ticker of tickers.values()) Object.assign(ticker, { bid: 0, ask: 0 });
    for (const record of records) {
      if (
        record.type === 'http' &&
        record.method === 'GET' &&
        new URL(record.url).pathname === '/api/v1/orders' &&
        record.status === 200
      ) {
        for (const order of parseRequestJson(record.body))
          orders.set(String(order.id), { c: order.clientId, y: order.postOnly });
      }
      if (record.type !== 'ws') continue;
      const { stream, data: d } = parseRequestJson(record.text);
      if (!d) continue;
      // TS globally throttles tickers. That
      // drops other markets' last updates. Rust coalesces at the flush boundary.
      // TS also reads P/q instead of the documented o/V ticker fields.
      const ticker = tickers.get(d.s);
      if (ticker) {
        if (stream?.startsWith('ticker.')) {
          const change =
            d.P !== undefined
              ? Number(d.P) * 100
              : Number(d.o) > 0
                ? ((Number(d.c) - Number(d.o)) / Number(d.o)) * 100
                : undefined;
          Object.assign(ticker, {
            last: Number(d.c),
            ...(change === undefined ? {} : { percentage: change }),
            volume: Number(d.v),
            quoteVolume: Number(d.q ?? d.V),
          });
        }
        if (stream?.startsWith('trade.')) ticker.last = Number(d.p);
        if (stream?.startsWith('bookTicker.')) Object.assign(ticker, { bid: Number(d.b), ask: Number(d.a) });
        // TS mistakes the first changed depth level for the best quote.
        // bookTicker is the venue's authoritative best bid/ask stream; null
        // sides mean empty (zero), which TS incorrectly leaves stale.
      }
      if (stream?.startsWith('account.orderUpdate')) {
        const previous = orders.get(String(d.i));
        if (!previous || (d.E ?? 0) >= (previous.E ?? 0)) orders.set(String(d.i), d);
        // TS emits duplicates on reconnect; one execution must cross once.
        if (d.e === 'orderFill' && Number(d.l) > 0) {
          const key = d.t == null ? `${d.i}:${d.E}:${d.z}` : `${d.i}:${d.t}`;
          if (!executions.has(key)) {
            executions.add(key);
            fills.push({
              id: String(d.i),
              symbol: d.s,
              side: d.S === 'Bid' ? 'buy' : 'sell',
              amount: Number(d.l),
              price: Number(d.L ?? d.P ?? d.p ?? 0),
            });
          }
        }
      }
      if (stream?.startsWith('account.positionUpdate') && (!positions.has(d.s) || d.E >= positions.get(d.s).E))
        positions.set(d.s, d);
    }
    if (!ts.account) return;
    // TS returns Hedged but never publishes its fixed mode in Store options.
    ts.account.hedged = true;
    for (const order of ts.account.orders) {
      const d = orders.get(order.id);
      if (!d) continue;
      // TS drops native client IDs on both REST and stream mapping.
      order.clientOrderId = d.c == null ? null : String(d.c);
      // REST postOnly / WS y are separate from GTC. TS loses maker protection.
      if (d.y) order.timeInForce = 'PostOnly';
      // Y is trigger quantity; Q is reverse-market quote quantity. A failed
      // protection order is terminal, not open.
      if (d.P !== undefined && d.Y !== undefined) {
        order.amount = Number(d.Y);
        order.remaining = order.amount - order.filled;
      }
      if (d.X === 'TriggerFailed') order.status = 'canceled';
      // TS drops the trigger price source, so amend falls back to MarkPrice.
      if (d.P !== undefined && d.B)
        order.trigger = { MarkPrice: 'mark_price', LastPrice: 'last_price', IndexPrice: 'index_price' }[d.B] ?? null;
    }
    for (const position of ts.account.positions) {
      const d = positions.get(position.symbol);
      // B is entry; b is breakeven. Do not overwrite entry with breakeven.
      if (d?.B !== undefined && Number(d.q) !== 0) position.entryPrice = Number(d.B);
    }
    ts.account.fills = fills;
  },
};

for (const alias of Object.keys(ORDERLY_BROKERS)) KNOWN_TS_FIXES[alias] = KNOWN_TS_FIXES.orderly;
KNOWN_TS_FIXES['lighter-robinhood'] = KNOWN_TS_FIXES.lighter;

const normalizeBingxPublic = (ts) => {
  // KNOWN_TS_FIXES: BingX turns precision=0 into 0.1 instead of the native
  // integer step 1. Correct that precision and its synthetic ticker touch only.
  if (values.venue === 'bingx') {
    const contracts = new Map();
    for (const record of records) {
      if (record.type !== 'http' || new URL(record.url).pathname !== '/openApi/swap/v2/quote/contracts') continue;
      const response = JSON.parse(record.body);
      if (response.code !== 0 || !Array.isArray(response.data)) continue;
      for (const contract of response.data) contracts.set(contract.symbol, contract);
    }
    const zeroPrecision = (value) =>
      (typeof value === 'number' || (typeof value === 'string' && value.trim() !== '')) && Number(value) === 0;
    const prices = new Set();
    for (const market of ts.markets) {
      const source = contracts.get(market.id);
      for (const [field, nativeField] of [
        ['amount', 'quantityPrecision'],
        ['price', 'pricePrecision'],
      ]) {
        if (market.precision[field] === 0.1 && zeroPrecision(source?.[nativeField])) {
          market.precision[field] = 1;
          if (field === 'price') prices.add(market.symbol);
        }
      }
    }
    // TS parses non-trading '-' placeholders into NaN; Rust stores finite zeros.
    for (const ticker of ts.tickers) {
      for (const field of ['percentage', 'volume', 'quoteVolume']) if (ticker[field] === null) ticker[field] = 0;
      if (!prices.has(ticker.symbol)) continue;
      ticker.bid = Math.max(ticker.last - 1, 0);
      ticker.ask = ticker.last + 1;
    }
  }
  // KNOWN_TS_FIXES: BingX's internal feed sends asks descending; TS forwards
  // that order. Consumers require best ask first, so Rust sorts full snapshots.
  if (values.venue === 'bingx') {
    for (const book of Object.values(ts.books)) {
      book.bids.sort((a, b) => b[0] - a[0]);
      book.asks.sort((a, b) => a[0] - b[0]);
    }
  }
};
const fixCryptocomAccount = (account) => {
  // TS never sets the hedged store flag despite mapping positions as hedged.
  account.hedged = true;
  // TS omits user.trade entirely. Derive the execution oracle from documented
  // taped trade rows, independently of Rust's account output.
  const tape = tapeRecords;
  const seen = new Set();
  account.fills = [];
  for (const record of tape) {
    if (record.type !== 'ws' || !record.url.includes('/exchange/v1/user')) continue;
    const result = parseRequestJson(record.text).result;
    if (result?.channel !== 'user.trade') continue;
    for (const row of result.data) {
      const id = String(row.trade_id);
      if (seen.has(id)) continue;
      seen.add(id);
      account.fills.push({
        id: String(row.order_id),
        symbol: row.instrument_name,
        side: row.side.toLowerCase(),
        amount: Number(row.traded_quantity),
      });
    }
  }
  // Further deliberate fixes are exercised by Rust regressions: failed reads
  // retain state; client_oid survives mapping; terminal/stale frames cannot
  // reopen orders/positions; REST retains live position enrichment; post-only
  // uses exec_inst; account-wide leverage uses account_id + integer leverage;
  // ignored brackets/opening triggers reject before writes; caller ids use a
  // conservative 36-byte ceiling without JS string emulation.
  // Public throttle and public-only subscription corrections are the
  // afterTsFrame and prepareTsReady hooks in venues.mjs.
};

function applyTsCorrectionsToShadow(ts) {
  const fix =
    KNOWN_TS_FIXES[values.venue] ??
    (['bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(values.venue)
      ? KNOWN_TS_FIXES.bybit
      : undefined);
  // A fixer receives only TS and input records, never the candidate answer.
  if (typeof fix === 'function') fix(ts);
  if (values.venue === 'kraken') KNOWN_TS_FIXES.kraken.output(ts, tapeRecords);
  if (values.venue === 'bybitspot') {
    for (const market of ts.markets) {
      const lot = instruments.get(market.symbol)?.lotSizeFilter;
      if (!lot) continue;
      market.precision.amount = Number(lot.qtyStep ?? lot.basePrecision);
      const maxima = [lot.maxLimitOrderQty, lot.maxMarketOrderQty, lot.maxOrderQty]
        .map(Number)
        .filter((n) => Number.isFinite(n) && n > 0);
      if (maxima.length) market.limits.amount.max = Math.min(...maxima);
    }
  }
  if (ts.account && values.venue === 'bybitinverse') {
    const upnl = ts.account.positions.reduce(
      (sum, p) => sum + (p.contracts > 0 ? (p.unrealizedPnl * p.notional) / p.contracts : 0),
      0,
    );
    ts.account.balance.total = Math.round((ts.account.balance.total + ts.account.balance.upnl - upnl) * 1e8) / 1e8;
    ts.account.balance.upnl = upnl;
  }
  if (['bybitinverse', 'bybitspot'].includes(values.venue)) {
    const privateRecord = tapeRecords.find(
      (record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/private',
    );
    const privateUrl = privateRecord && new URL(privateRecord.url);
    ts.subscriptions = normalizeTsBybitSubscriptions(
      values.venue,
      ts.subscriptions,
      privateUrl && `${privateUrl.origin}${privateUrl.pathname}`,
    );
  }
  if (ts.account) {
    if (values.venue === 'cryptocom') {
      fixCryptocomAccount(ts.account);
      const feeds = new Set(ts.subscriptions);
      const suffix = ' {"method":"subscribe","params":{"channels":["user.order"]}}';
      for (const key of ts.subscriptions) {
        if (!key.endsWith(suffix)) continue;
        for (const required of subscriptionKeys(
          key.slice(3, -suffix.length),
          { method: 'subscribe', params: { channels: ['user.trade'] } },
          new Set(),
          { cryptoProtocol: true },
        ))
          feeds.add(required);
      }
      ts.subscriptions = [...feeds].sort();
    }
    if (['lighter', 'lighter-robinhood'].includes(values.venue)) {
      KNOWN_TS_FIXES.lighter.fills(ts.account, tapeRecords);
      KNOWN_TS_FIXES.lighter.commands(ts.commands, tapeRecords);
    }
  }
  // KNOWN_TS_FIXES: KuCoin TS has no futures history implementations. The
  // synthetic tape declares independent documented query expectations; never
  // derive this reference from the Rust result.
  if (values.venue === 'kucoin')
    tapeRecords
      .filter((r) => r.type === 'command')
      .forEach((record, index) => {
        if (record.expected !== undefined) {
          if (!['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(record.op))
            throw new Error('KuCoin expected oracle is restricted to missing TS query methods');
          ts.commands[index] = { ok: record.expected };
        }
      });
  normalizeBingxPublic(ts);
  if (values.venue === 'kucoin') {
    ts.requests = expectedKucoinRequests(tapeRecords, ts.requests);
  }
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom')
    for (const ticker of ts.tickers) {
      for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
        if (ticker[field] === null) ticker[field] = 0;
      }
    }
}
const legacyFillVenues = new Set([
  'coinbase', 'coinbasespot', 'orderly', ...Object.keys(ORDERLY_BROKERS),
  'phemex', 'kraken', 'bingx', 'backpack', 'cryptocom', 'lighter', 'lighter-robinhood',
]);
function applyTsCorrections(ts) {
  if (legacyFillVenues.has(values.venue) && Array.isArray(ts.account?.fills))
    return applyLegacyFillOracle(ts, applyTsCorrectionsToShadow);
  return applyTsCorrectionsToShadow(ts);
}
return { KNOWN_TS_FIXES, applyTsCorrections };
}
