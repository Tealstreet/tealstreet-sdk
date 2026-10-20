import { fixBinanceSpotFills, fixBinanceFilledStatus } from './binance-protocol.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';
// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { applyKucoinRequestContracts } from './kucoin-request-contracts.mjs';
import { fixCoinbase, COINBASE_TS_FIXES } from './known-ts-fixes-coinbase.mjs';
import { memoryMetricErrors } from './memory.mjs';
import { fixWeexConditionals, fixLighterFills, fixLighterCommands, fixKrakenOutput, fixPhemexAccount } from './known-ts-fixes.mjs';
import { parseRequestJson } from './request-json.mjs';
import { normalizeTsBybitSubscriptions } from './bybit-subscriptions.mjs';
import { requestKey, callerIdsOf, subscriptionKeys } from './requests.mjs';
import { venueOf } from './venues.mjs';
import { fixOrderly, ORDERLY_TS_FIXES } from './known-ts-fixes-orderly.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string', default: '3' },
    'no-egress': { type: 'boolean', default: false },
  },
});
const outDir = `sdk-rs/harness/out/${values.venue}-${values.tape.split('/').at(-1).replaceAll('.', '-')}-${process.pid}`;
mkdirSync(outDir, { recursive: true });
console.log(`Replay output: ${outDir}`);

const rawTape = readFileSync(values.tape);
const smokeResult = assertLiveSmokeComplete((values.tape.endsWith('.gz') ? gunzipSync(rawTape) : rawTape).toString('utf8').split('\n').filter(Boolean).map(JSON.parse));
if (smokeResult) values.books = smokeResult.symbol;

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125_000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } });
  if (result.status !== 0) throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  const output = JSON.parse(readFileSync(out, 'utf8'));
  if (smokeResult && (!Array.isArray(output.commands) || output.commands.length !== smokeResult.commands.length || output.commands.some((result) => !result || !Object.hasOwn(result, 'ok') || Object.hasOwn(result, 'error')))) throw new Error(`${impl} live-smoke replay did not successfully complete every lifecycle command`);
  return output;
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const tapeBytes = readFileSync(values.tape);
const tape = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes).toString('utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
const canonical = (value) => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object'
  ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, canonical(v)])) : value;
const equal = (a, b) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));

const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    const left = JSON.stringify(ta.get(symbol));
    const right = JSON.stringify(tb.get(symbol));
    if (!equal(ta.get(symbol), tb.get(symbol))) differences.push(`${label} ${symbol}: ts=${left} rust=${right}`);
  }
};

const runs = Number(values.runs);
const results = { ts: [], rust: [] };
for (let run = 0; run < runs; run++) {
  for (const impl of ['ts', 'rust']) results[impl].push(replay(impl, run));
}
const venue = venueOf(values.venue);
const isPrivate = tape.find((record) => record.type === 'config')?.private ?? tape.find((record) => record.type === 'meta')?.private ?? tape.some((record) =>
  venue.isPrivateRecord ? venue.isPrivateRecord(record) : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)),
);
// Paired production reference repairs for the new ports, without output waivers:
// GRVT keeps sparse ticker fields, exact native units/IDs, session refresh and
// private execution state; failed/stale reads do not clear live account state.
// Polymarket retains array depth/absolute changes and event watermarks, complete
// Gamma discovery, owned executions and cancellation/replacement race fences.
// Breakout selects the exact account, retains same-symbol positionCode lots,
// validates portfolio sections atomically, fences session/write-stale reads,
// polls public quotes, returns string order IDs and sends If-Match cancellation.
// Its paired TS adapter also coalesces startup account discovery and publishes
// the first fetched quotes to existing book listeners, preserving poll phase.
// Coinbase's explicit independent input oracle is in known-ts-fixes-coinbase.mjs.
// Every port still compares all intermediate book timestamps/hashes unchanged.
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
// BingX additionally fixes write/read defects covered by native contract tests:
// - closePosition=true STOP_MARKET/TAKE_PROFIT_MARKET requires quantity.
// - A rejected/ambiguous entry must stop its remaining bracket placement jobs.
// - fillHistory uses startTs/endTs/pageSize/pageIndex, not TS's ignored params.
// - Contradictory placement IDs or rejected ACK status must not advance jobs.
// - Delayed placement/replacement ACKs must not reopen newer private state.
// - Trigger replacement checks its selected source; TS always checks mark.
// - Attached SL/TP keeps per-leg workingType; TS spreads the untyped entry.

//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
//  - Bybit: TS drops the reject reason of a post-only order cancelled for taking
//    liquidity (`EC_PostOnlyWillTakeLiquidity`); Rust reports it as PostOnly.
//  - Bybit: TS can round split quantities away from the requested total;
//    Rust uses maximum-size lots and the exact remainder.
//  - Bybit: TS falls back from failed batch writes to single creates, even
//    after ambiguous HTTP/transport failures; Rust never resends those writes.
//  - Bybit: TS may drop the initial ticker snapshot before a slow REST seed;
//    Rust installs the seed before connecting. These cases have native regression
//    coverage; the committed replay tapes require no normalization for them.
//  - Bybit: with no mark price, TS can add a second Partial stop on amend;
//    Rust amends the existing native order ID.
// OKX full-depth correction is also applied to the TS oracle: its old
// last-trade filter permanently deleted valid levels when the trade lagged.
// Paired production fixes compare outputs without answer normalization:
// Binance PM symbol cancellation attempts both regular and conditional families.
// USD-M/PM partial failures preserve the failed family, completed fills stay
// closed, and later placements do not inherit an earlier cancel-all ACK.
// Binance EXPIRED_IN_MATCH is canceled, like EXPIRED. Binance/OKX client-ID
// lookups bypass secondary-account caches and query the primary account;
// a secondary account's absence cannot prove the primary order was not placed.
// GRVT retains mixed-product identities and validates raw open orders before
// exact-instrument cancellations; ambiguous ACKs cannot retarget a client ID.
// Phemex REST totals exclude position UPnL, matching its stream totals, while
// authoritative revision fences preserve newer private state.
// KuCoin Spot fixes stay in production: native execution deduplication, stale
// read and malformed account-row fences, and URL-encoded cancellation IDs.
// Both replay outputs are compared directly without a KuCoin Spot repair.
const fixBackpack = (ts) => {
    const bytes = readFileSync(values.tape);
    const records = (values.tape.endsWith('.gz') ? gunzipSync(bytes) : bytes)
      .toString()
      .split('\n')
      .filter(Boolean)
      .map(JSON.parse);
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
        for (const order of JSON.parse(record.body))
          orders.set(String(order.id), { c: order.clientId, y: order.postOnly });
      }
      if (record.type !== 'ws') continue;
      const { stream, data: d } = JSON.parse(record.text);
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

};
// Spot aggregate !ticker@arr/!bookTicker streams were retired; the TS replay
// adapter sends documented per-symbol ticker shards and uses TS's own handlers.
// Signature-only subscriptions use session.subscriptions, not session.status.
// COIN-M's documented fixes also have native regression proofs: exact split
// totals, plain-trigger/caller-ID whole-batch rejection, post-only amendment,
// cumulative-fill/terminal replay fences and malformed openOrders retention.
// TS cancel-all marks already FILLED rows canceled; use explicit tape events.
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

// TxFlow compares both implementations directly; no venue-specific output repair.
const KNOWN_TS_FIXES = (account, rustAccount) => {
  if (values.venue === 'blofin') {
    // TS can retain the zero-size placeholder's margin. Derive the repair
    // from its own normalized cross-position notional and leverage.
    for (const position of account.positions) {
      if (position.marginSize === 0 && position.marginType === 'cross' && position.leverage > 0)
        position.marginSize = position.notional / position.leverage;
    }
    // No remaining positions implies no position margin or unrealized PnL.
    if (!account.positions.length) {
      account.balance.used = 0;
      account.balance.upnl = 0;
    }
    return;
  }
  if (values.venue === 'mexc') {
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
    return;
  }
  if (values.venue === 'bingx') {
    // BingX TS emits a terminal cumulative fill, ignores partial executions,
    // and drops its terminal watermark so TRADE_UPDATE can repeat that fill.
    // Use the taped per-execution oracle; Rust is still compared against it.
    const watermark = new Map();
    const fills = [];
    for (const record of tape) {
      if (record.type !== 'ws' || !record.url?.includes('?listenKey=')) continue;
      let event;
      try { event = JSON.parse(record.text, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^-?\d+$/.test(context.source) ? context.source : value); } catch { continue; }
      if (!['ORDER_TRADE_UPDATE', 'TRADE_UPDATE'].includes(event.e)) continue;
      const o = event.o;
      if (!['PARTIALLY_FILLED', 'FILLED'].includes(o.X)) continue;
      const id = String(o.i);
      const cumulative = Number(o.z);
      const previous = watermark.get(id) ?? 0;
      const amount = Math.max(0, cumulative - previous);
      watermark.set(id, Math.max(previous, cumulative));
      if (amount > 0) fills.push({ id, symbol: o.s.replaceAll('-', ''), side: o.S === 'BUY' ? 'buy' : 'sell', amount: Number(o.l) > 0 ? Math.min(amount, Number(o.l)) : amount });
    }
    account.fills = fills;
    // TS's private mapper reads closePosition but misses the native cp alias.
    for (const record of tape) {
      if (record.type !== 'ws') continue;
      let event; try { event = JSON.parse(record.text, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^-?\d+$/.test(context.source) ? context.source : value); } catch { continue; }
      const row = account.orders.find((o) => o.id === String(event.o?.i));
      if (row && event.o.cp !== undefined) row.close = event.o.cp === true || event.o.cp === 'true';
    }
    return;
  }

  if (values.venue === 'weex') {
    // Opening STOP/TP rows are entries and their trigger is not execution price.
    fixWeexConditionals(account, tape);
    // Native regressions also cover failed read retention, stale private/depth
    // rejection, fill ID deduplication, missing placement IDs, rejected cancels,
    // per-symbol margin mode, safe replacement, and incomplete income cursors.
    return;
  }
  if (values.venue === 'bitunix') {
    // Bitunix fixes: failed reconciliation preserves state; live cumulative fills
    // emit once; history dedupes by tradeId rather than collapsing equal fills;
    // Split sizes conserve the adjusted amount in native integer steps, and
    // reordered order events cannot regress cumulative fills or filled terminals.
    // Native regressions pin both TS defects without output waivers here.
    // Split orders use distinct generated client IDs; cancel replies
    // require per-order success; replacements reconcile final remaining quantity.
    // Full-position TP/SL batches coalesce sibling edits. Setting ACKs fence
    // stale reads; cancel ACKs preserve newer terminal events.
    // Native-only cancellation admission rejects absent-store IDs before writes;
    // TS takes full Order objects, so no reference result is rewritten for it.
    // TS never emits Bitunix live fills. This independent tape oracle validates
    // cumulative quantities/costs; it does not copy anything from Rust's output.
    const totals = new Map();
    for (const record of tape.filter((r) => r.type === 'http' && new URL(r.url).pathname.endsWith('/trade/get_pending_orders') && (r.after ?? 0) === 0)) {
      const data = JSON.parse(record.body).data;
      for (const row of Array.isArray(data) ? data : data.orderList) totals.set(String(row.orderId), [Number(row.tradeQty), Number(row.tradeQty) * Number(row.avgPrice ?? row.price)]);
    }
    account.fills = [];
    for (const record of tape.filter((r) => r.type === 'ws')) {
      const message = JSON.parse(record.text);
      if (message.ch !== 'order') continue;
      const row = message.data;
      const id = String(row.orderId);
      const [previous, previousCost] = totals.get(id) ?? [0, 0];
      const cumulative = Number(row.dealAmount ?? 0);
      const delta = cumulative - previous;
      if (delta > 0) {
        const cost = Number(row.averagePrice) > 0 ? Number(row.averagePrice) * cumulative : previousCost + delta * Number(row.price);
        account.fills.push({ id, symbol: row.symbol, side: row.side.toLowerCase(), amount: delta, price: (cost - previousCost) / delta });
        totals.set(id, [cumulative, cost]);
      }
    }
    return;
  }
  if (!['binance', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(values.venue)) return;
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
  }

};
// Bitrue's paired TS corrections preserve native quantities, cumulative fills,
// execution prices, terminal state, cancellation identities and heartbeat echoes.
// Native regressions additionally pin split conservation, conditional identity,
// protection provenance and failed/incomplete history; no oracle values change.
// WOO repairs are paired in the TS reference itself, without oracle rewriting:
// exact integer IDs, cumulative/terminal order state, nested algo executions,
// per-execution deduplication, atomic failed-page snapshots, protection provenance,
// tick splitting, cancellation families, hedge mode and signed V1 income fees.
// Rust additionally walks V3 fill time windows; TS still reads one page.
// Lighter's independent tape oracle fixes duplicated/missed TS execution notifications
// and acknowledges retaining net-only mode; native output is never its input.
KNOWN_TS_FIXES.hyperliquidspot = {
    // Paired reference fixes retain native spot IDs for coinless contexts and
    // return string IDs for both resting and filled placement ACKs. Native
    // tests additionally pin spot-only safety paths without rewriting economics.
    notes: [
      'Join coinless spot contexts positionally only when the entire context array is coinless and exactly matches universe length.',
      'Resolve both metadata token indices so native pairs with a shared base retain distinct quote symbols and action/book identities.',
      'Convert non-USD quote prices before wallet valuation, prefer direct USDC and reject canonical-token cycles while retaining unpriced raw holdings.',
      'Use the spot native websocket endpoint and supported book precision subscriptions (null and 2 through 5).',
      'Return the SDK string-array contract for resting and immediately filled orders.',
      'Keep execution tid as the TS fill id and expose native oid as orderId for chaser tracking and native fill parity.',
      'Subscribe to documented spotState balance updates and remove ignored feeds and perpetual-only activeAssetData subscriptions.',
      'Keep absent spot oracle/funding fields finite instead of emitting NaN.',
      'Use spot asset indices and 8-minus-size-decimals price precision with no perp builder or reduce-only flags.',
      'Keep accepted orders available for immediate cancel/amend despite the TS spot fetchOrders stub.',
      'Apply live full/partial executions before a delayed resting acknowledgement without resurrecting the filled native order.',
      'Reject unsupported spot leverage, margin transitions and standalone trigger placement before a write.',
      'Deduplicate native live executions and suppress historical fill snapshots.',
      'Retry failed bootstrap reads without replacing a valid market table with an empty snapshot.',
    ],
};
KNOWN_TS_FIXES.krakenspot = {
    // Public fixes are paired in TS; independent native tests cover stricter
    // execution, retry and amendment paths. Replay output stays unmodified.
    notes: [
      'Translate AssetPairs XBT and XDG/XXDG identities to accepted WS v2 BTC/DOGE wire symbols while preserving canonical store symbols.',
      'Value normalized BTC holdings through the market identity even when the SDK ticker uses XBT/USD.',
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
};
KNOWN_TS_FIXES.lighter = { fills: fixLighterFills, commands: fixLighterCommands };
KNOWN_TS_FIXES.woo = [];
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
// Paradex fixes are paired in production TS: full depth/sequence recovery,
// mandatory SBE1:2, signed payload/auth headers, exact execution identity,
// candle ordering, margin settings and safe order amendments. No output rewrite.
// Phemex execution IDs, cumulative exposure and flat settings derive only
// from recorded native rows; full-depth production TS books stay unmodified.
KNOWN_TS_FIXES.phemex = [];
KNOWN_TS_FIXES.paradex = [];
KNOWN_TS_FIXES.apexomni = [];
KNOWN_TS_FIXES.kraken = {
    output: fixKrakenOutput,
    fixes: [
      'TS production now retains full depth, rejects stale/gapped updates, and resubscribes for recovery; exact intermediate timestamp/hash checks stay unchanged.',
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
  };

KNOWN_TS_FIXES.asterdex = {
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
  };
KNOWN_TS_FIXES.kucoin = [
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
    'unbridged recovery snapshots cannot mutate previously published book references (production TS fixed; full event hashes remain exact)',
  ];
// Fresh TS private rows retain native/cached TIF and default to GTC, matching
// REST and client-ID lookup; command results remain compared without repair.
KNOWN_TS_FIXES.bitrue = [];
KNOWN_TS_FIXES.bitget = {
  // Each correction is covered by documented-payload Rust tests. Normal
  // tapes still compare unchanged; these notes do not rewrite mismatches.
  notes: [
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
};

KNOWN_TS_FIXES.okx = [
  'full books retain levels past a stale last trade',
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
];
// KNOWN_TS_FIXES: the history composite-key map drops distinct executions
// with equal order/time/price/size/side. Reconstruct only that specific loss
// from documented trade IDs on the tape, independently of Rust's results.
const fixHistoricalFills = (commands) => {
  if (values.venue !== 'bitunix') return;
  const requests = tape.filter((r) => r.type === 'command');
  requests.forEach((request, index) => {
    if (request.op !== 'fetchFills' || !Array.isArray(commands[index]?.ok)) return;
    const response = tape.find((r) => r.type === 'http' && r.url.includes('/trade/get_history_trades?') &&
      new URL(r.url).searchParams.get('orderId') === request.orderId);
    if (!response) return;
    const rows = JSON.parse(response.body).data.tradeList;
    const executions = rows.map((r) => ({ id: r.tradeId, orderId: r.orderId, symbol: r.symbol, timestamp: Math.floor(r.ctime / 1000),
      side: r.side.toLowerCase(), price: Number(r.price), qty: Number(r.qty), commission: Math.abs(Number(r.fee)), commissionAsset: r.marginCoin }));
    const key = (e) => JSON.stringify([e.orderId, e.timestamp, e.price, e.qty, e.side]);
    const collapsed = [...new Map(executions.map((e) => [key(e), e])).values()].sort((a, b) => b.timestamp - a.timestamp);
    if (collapsed.length !== executions.length && equal(commands[index].ok, collapsed))
      commands[index].ok = executions.sort((a, b) => b.timestamp - a.timestamp);
  });
};
const normalizeBingxPublic = (ts) => {
  // KNOWN_TS_FIXES: BingX turns precision=0 into 0.1 instead of the native
  // integer step 1. Correct that precision and its synthetic ticker touch only.
  if (values.venue === 'bingx') {
    const contracts = new Map();
    for (const record of tape) {
      if (record.type !== 'http' || new URL(record.url).pathname !== '/openApi/swap/v2/quote/contracts') continue;
      const response = JSON.parse(record.body);
      if (response.code !== 0 || !Array.isArray(response.data)) continue;
      for (const contract of response.data) contracts.set(contract.symbol, contract);
    }
    const zeroPrecision = (value) => (typeof value === 'number' || typeof value === 'string' && value.trim() !== '') && Number(value) === 0;
    const prices = new Set();
    for (const market of ts.markets) {
      const source = contracts.get(market.id);
      for (const [field, nativeField] of [['amount', 'quantityPrecision'], ['price', 'pricePrecision']]) {
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
      book.bids.sort((a,b) => b[0]-a[0]);
      book.asks.sort((a,b) => a[0]-b[0]);
    }
  }
};
const fixCryptocomAccount = (account) => {
    // TS never sets the hedged store flag despite mapping positions as hedged.
    account.hedged = true;
    // TS omits user.trade entirely. Derive the execution oracle from documented
    // taped trade rows, independently of Rust's account output.
    const tapeRecords = tape;
    const seen = new Set();
    account.fills = [];
    for (const record of tapeRecords) {
      if (record.type !== 'ws' || !record.url.includes('/exchange/v1/user')) continue;
      const result = parseRequestJson(record.text).result;
      if (result?.channel !== 'user.trade') continue;
      for (const row of result.data) {
        const id = String(row.trade_id);
        if (seen.has(id)) continue;
        seen.add(id);
        account.fills.push({ id: String(row.order_id), symbol: row.instrument_name, side: row.side.toLowerCase(), amount: Number(row.traded_quantity) });
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
for (let run = 0; run < runs; run++) {
  const result = { ts: results.ts[run], rust: results.rust[run] };
  const { ts, rust } = result;
  if (values.venue === 'binancespot') fixBinanceSpotFills(ts.account, tape);
  if (['binancespot','binanceinverse'].includes(values.venue)) fixBinanceFilledStatus(ts.account, tape, values.venue === 'binancespot');
  if (values.venue === 'kraken') fixKrakenOutput(ts, tape);
  if (venue.family === 'coinbase') fixCoinbase(ts, tape);
  if (values.venue === 'phemex') {
    if (ts.account) fixPhemexAccount(ts.account, tape);
    for (const ticker of ts.tickers) if (ticker.percentage === null) ticker.percentage = 0;
  }
  if (ts.frames !== rust.frames) differences.push(`run ${run} frames: ts=${ts.frames} rust=${rust.frames}`);
  if (values.venue === 'backpack') fixBackpack(ts);
  if (venue.family === 'orderly') fixOrderly(ts, tape);
  if (venue.family === 'orderly' && !equal(ts.requests, rust.requests))
    differences.push(`requests differ: ts=${JSON.stringify(ts.requests)} rust=${JSON.stringify(rust.requests)}`);
  // Phase-9 native regressions additionally correct TS: account-wide history
  // limits before joining newer inverse/options sources; ignored IV on ordinary
  // derivatives; generic option tickers omitted and option books routed through
  // the linear feed; malformed supplied option WS Greeks coerced to zero
  // instead of retaining proven position/Greek state. Dedicated API tests compare
  // options terms/queries/signing, and assert native option depth25 separately.
  if (venue.family === 'bybit') {
    if (ts.account) {
      const rest = tape.find((record) => record.type === 'config')?.options?.endpoints?.rest
        ?? (values.venue === 'bybitdemo' ? 'https://api-demo.bybit.com' : 'https://api.bybit.com');
      const metadataRequest = requestKey('GET', `${rest}/v5/user/query-api`, undefined, new Set(), venue);
      ts.requests = ts.requests.filter((key) => key !== metadataRequest);
      if (values.venue !== 'hyrotrader') {
        const feeds = new Set(ts.subscriptions);
        for (const key of ts.subscriptions) {
          const match = /^([1-9]\d*) WS (\S+) \{"args":\["order"\],"op":"subscribe"\}$/.exec(key);
          if (!match || new URL(match[2]).pathname !== '/v5/private') continue;
          for (const required of subscriptionKeys(match[2], { op: 'subscribe', args: ['execution', 'wallet'] }, new Set(), { fullSocketIdentity: true }))
            feeds.add(`${match[1]} ${required}`);
        }
        ts.subscriptions = [...feeds].sort();
      }
      const latest = new Map();
      for (const record of tape) {
        if (record.type !== 'ws') continue;
        const event = JSON.parse(record.text);
        if (event.topic === 'order') for (const row of event.data) latest.set(String(row.orderId), row);
      }
      for (const order of ts.account.orders) {
        if (order.rejectReason === null && order.status === 'canceled'
          && latest.get(order.id)?.rejectReason === 'EC_PostOnlyWillTakeLiquidity') order.rejectReason = 'PostOnly';
      }
    }
    if (['bybitinverse', 'bybitspot'].includes(values.venue)) {
      const privateRecord = tape.find((record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/private');
      const privateUrl = privateRecord && new URL(privateRecord.url);
      ts.subscriptions = normalizeTsBybitSubscriptions(values.venue, ts.subscriptions,
        privateUrl && `${privateUrl.origin}${privateUrl.pathname}`);
    }
  }
  if (isPrivate && ts.account && rust.account) {
    if (values.venue === 'cryptocom') fixCryptocomAccount(ts.account);
    KNOWN_TS_FIXES(ts.account, rust.account);
    fixHistoricalFills(ts.commands);
    if (['lighter', 'lighter-robinhood'].includes(values.venue)) {
      KNOWN_TS_FIXES.lighter.fills(ts.account, tape);
      KNOWN_TS_FIXES.lighter.commands(ts.commands, tape);
    }
    if (values.venue === 'bybitinverse') {
      const upnl = ts.account.positions.reduce((sum, position) => sum
        + (position.contracts > 0 ? position.unrealizedPnl * position.notional / position.contracts : 0), 0);
      ts.account.balance.total = Math.round((ts.account.balance.total + ts.account.balance.upnl - upnl) * 1e8) / 1e8;
      ts.account.balance.upnl = upnl;
    }
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(ts.account[field]);
      const right = JSON.stringify(rust.account[field]);
      if (!equal(ts.account[field], rust.account[field])) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
  }
  if (values.venue === 'kucoin') {
    const commands = tape.filter((record) => record.type === 'command');
    if (ts.commands.length !== commands.length || rust.commands.length !== commands.length)
      throw new Error('KuCoin missing command result');
    commands.forEach((record, index) => {
      if (record.expected === undefined) return;
      if (!['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(record.op))
        throw new Error('KuCoin expected oracle is restricted to missing TS query methods');
      ts.commands[index] = { ok: record.expected };
    });
    const requests = applyKucoinRequestContracts(tape, ts.requests, rust.requests);
    ts.requests = requests.tsRequests;
    rust.requests = requests.rustRequests;
    if (!equal(ts.requests, rust.requests))
      differences.push(`requests differ: ts=${JSON.stringify(ts.requests)} rust=${JSON.stringify(rust.requests)}`);
  }
  if (!equal(ts.commands, rust.commands)) differences.push(`command results: ts=${JSON.stringify(ts.commands)} rust=${JSON.stringify(rust.commands)}`);
  for (const impl of ['ts', 'rust']) {
    const output = result[impl];
    if (output.invalidSignatures) differences.push(`${impl}: ${output.invalidSignatures} invalid signatures`);
    if (Boolean(output.account) !== isPrivate) differences.push(`${impl}: account presence differs from tape classification`);
    for (const error of memoryMetricErrors(output.metrics)) differences.push(`${impl}: ${error}`);
    for (const field of ['retainedBytes', 'wasmBytes']) {
      if (!Number.isFinite(output.metrics[field]) || output.metrics[field] < 0) {
        differences.push(`${impl}: invalid ${field} (${output.metrics[field]}); baseline is invalid`);
      }
    }
    for (const symbol of values.books.split(',').filter(Boolean)) {
      if (!output.books[symbol]) differences.push(`${impl}: requested book ${symbol} is missing`);
    }
    if (output.unusedWrites) differences.push(`${impl}: ${output.unusedWrites} taped mutation(s) never sent`);
    for (const error of output.protocolErrors ?? []) differences.push(`${impl}: ${error}`);
    if (!result[impl].markets.length || !result[impl].tickers.length) differences.push(`${impl}: empty public seed`);
    if (result[impl].signatures?.length) differences.push(`${impl}: ${result[impl].signatures.join('; ')}`);
    if (result[impl].socketErrors?.length) differences.push(`${impl}: ${result[impl].socketErrors.join('; ')}`);
    if (result[impl].unserved) differences.push(`${impl}: ${result[impl].unserved} unserved requests`);
  }
  for (const field of ['outbound', 'subscriptions', 'candles', 'bookEvents', 'wsRequests']) {
    if (field === 'outbound' && venue.family === 'bybit') {
      const selected = (impl) => new Set(tape.filter((record) => record.type === 'http' && record.impl === impl)
        .map((record) => requestKey(record.method, record.url, record.requestBody, callerIdsOf(tape), venue)));
      if (!equal(ts.outbound?.filter((key) => !selected('ts').has(key)), rust.outbound?.filter((key) => !selected('rust').has(key))))
        differences.push(`outbound differs on run ${run + 1}`);
      continue;
    }
    if (!equal(ts[field], rust[field])) {
      const left = ts[field] ?? [];
      const right = rust[field] ?? [];
      const first = left.findIndex((row, index) => !equal(row, right[index]));
      differences.push(`${field}: ts count=${left.length} rust count=${right.length}, first difference ${first}: ts=${JSON.stringify(left[first])} rust=${JSON.stringify(right[first])}`);
    }
  }
  normalizeBingxPublic(ts);
  // KNOWN_TS_FIXES: Bitunix parses ISO ft with parseFloat (the year) and ignores
  // nft. The ticker contract uses the next funding timestamp in milliseconds.
  if (values.venue === 'bitunix') {
    const latest = new Map();
    for (const record of tape.filter((r) => r.type === 'ws')) {
      const m = JSON.parse(record.text);
      if (m.ch === 'price') latest.set(m.symbol, m.data);
    }
    for (const ticker of ts.tickers) {
      const price = latest.get(ticker.symbol);
      if (!price) continue;
      const value = price.nft || price.ft;
      const corrected = Number.isFinite(Number(value)) ? Number(value) : Date.parse(value);
      if (ticker.fundingTime === Number.parseFloat(price.ft)) ticker.fundingTime = corrected;
      else if (ticker.fundingTime !== corrected) differences.push(`unexpected TS funding time for ${ticker.symbol}`);
    }
  }
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom') for (const ticker of ts.tickers) {
    for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
      if (ticker[field] === null) ticker[field] = 0;
    }
  }
  if (values.venue === 'bybitspot') {
    const instruments = new Map(tape.filter((record) => record.type === 'http'
      && new URL(record.url).pathname === '/v5/market/instruments-info')
      .flatMap((record) => JSON.parse(record.body).result?.list ?? []).map((row) => [row.symbol, row]));
    for (const market of ts.markets) {
      const lot = instruments.get(market.symbol)?.lotSizeFilter;
      if (!lot) continue;
      market.precision.amount = Number(lot.qtyStep ?? lot.basePrecision);
      const maxima = [lot.maxLimitOrderQty, lot.maxMarketOrderQty, lot.maxOrderQty]
        .map(Number).filter((number) => Number.isFinite(number) && number > 0);
      if (maxima.length) market.limits.amount.max = Math.min(...maxima);
    }
  }
  if (venue.family === 'bybit') {
    const selected = (impl) => new Set(tape.filter((record) => record.type === 'http' && record.impl === impl)
      .map((record) => requestKey(record.method, record.url, record.requestBody, callerIdsOf(tape), venue)));
    const plans = { ts: selected('ts'), rust: selected('rust') };
    const left = ts.requests?.filter((key) => !plans.ts.has(key));
    const right = rust.requests?.filter((key) => !plans.rust.has(key));
    if (!Array.isArray(left) || !Array.isArray(right) || !equal(left, right))
      differences.push(`requests run ${run + 1}: ts=${JSON.stringify(left)} rust=${JSON.stringify(right)}`);
    const tsWrites = ts.writes?.filter((key) => !plans.ts.has(key));
    const rustWrites = rust.writes?.filter((key) => !plans.rust.has(key));
    if (!Array.isArray(tsWrites) || !Array.isArray(rustWrites) || ts.writeCount !== ts.writes.length
      || rust.writeCount !== rust.writes.length || !equal(tsWrites, rustWrites))
      differences.push(`HTTP write sequence run ${run + 1}: ts=${JSON.stringify(tsWrites)} rust=${JSON.stringify(rustWrites)}`);
    // The selected per-implementation rows are separately required by the
    // transport's single-use consumption audit, including ordered mutations.
  }
  compare('market', ts.markets, rust.markets);
  compare('ticker', ts.tickers, rust.tickers);
  for (const symbol of new Set([...Object.keys(ts.books), ...Object.keys(rust.books)])) {
    if (JSON.stringify(ts.books[symbol]) !== JSON.stringify(rust.books[symbol])) {
      differences.push(`book ${symbol} differs`);
    }
  }

  for (const symbol of values.books.split(',').filter(Boolean)) {
    if (!ts.books[symbol] || !rust.books[symbol]) differences.push(`requested book ${symbol} was not observed in both implementations`);
  }

}

const [ts, rust] = [results.ts[0], results.rust[0]];

const summary = (impl) => ({
  cpuMs: median(results[impl].map((r) => r.metrics.cpuMs)),
  steadyCpuMs: median(results[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(results[impl].map((r) => r.metrics.processCpuMs)),
  peakMB: median(results[impl].map((r) => r.metrics.peakBytes)) / 1e6,
  retainedMB: median(results[impl].map((r) => r.metrics.retainedBytes)) / 1e6,
  wasmMB: median(results[impl].map((r) => r.metrics.wasmBytes ?? 0)) / 1e6,
});
const t = summary('ts');
const r = summary('rust');
console.log(`frames ${ts.frames}, markets ${ts.markets.length}, tickers ${ts.tickers.length}, books ${Object.keys(ts.books).length}`);
const row = (label, a, b, unit = 'ms') =>
  console.log(`${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`);
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
row('sampled peak', t.peakMB, r.peakMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (isPrivate) console.log('(private tape: correctness only; CPU and memory are informational)');
if (venue.family === 'orderly') console.log(`(documented Orderly corrections: ${ORDERLY_TS_FIXES.length}; native protocol and independent oracles checked)`);
if (values.venue.startsWith('okx')) console.log(`(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`);

if (venue.family === 'coinbase') console.log(`(documented Coinbase corrections: ${COINBASE_TS_FIXES.length}; input-derived corrections only)`);
const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate) {
  if (!(r.cpuMs < t.cpuMs)) failures.push('Rust main-thread CPU is not below TS');
  if (!(r.steadyCpuMs < t.steadyCpuMs)) failures.push('Rust steady-state CPU is not below TS');
  if (!(r.retainedMB < t.retainedMB)) failures.push('Rust retained memory is not below TS');
  if (!(r.peakMB < t.peakMB)) failures.push('Rust sampled peak memory is not below TS');
}
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
