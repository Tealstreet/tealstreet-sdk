// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { memoryMetricErrors } from './memory.mjs';
import { fixWeexConditionals } from './known-ts-fixes.mjs';
import { parseRequestJson } from './request-json.mjs';
import { venueOf } from './venues.mjs';

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

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125_000 });
  if (result.status !== 0) throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
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
    // split orders use distinct generated client IDs; cancel replies
    // require per-order success; replacements reconcile final remaining quantity.
    // Full-position TP/SL batches coalesce sibling edits. Setting ACKs fence
    // stale reads; cancel ACKs preserve newer terminal events.
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
  if (!['binance', 'bybit'].includes(values.venue)) return;
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
  }

};
// Bitrue's paired TS corrections preserve native quantities, cumulative fills,
// execution prices, terminal state, cancellation identities and heartbeat echoes.
// Native regressions additionally pin split conservation, conditional identity,
// protection provenance and failed/incomplete history; no oracle values change.
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
    'Use cumulative executed size for orders and deduplicate executions.',
    'Do not emit an execution for an algo trigger acknowledgement.',
    'Split normal orders at floored max lots without losing requested volume.',
    'Send native crossed marginMode for Classic batches rather than cross.',
    'Resolve native modify identity using the fresh newClientOid.',
    'Never cancel-replace an amendment after an ambiguous transport/server result.',
    'Report incomplete or malformed financial history as a failed read.',
  ],
};

KNOWN_TS_FIXES.okx = [
  'full books retain levels past a stale last trade',
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
  'new account equity survives delayed snapshots and local balance ticks',
  'acknowledged leverage survives failed post-write snapshots',
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
  if (ts.frames !== rust.frames) differences.push(`run ${run} frames: ts=${ts.frames} rust=${rust.frames}`);
  if (values.venue === 'backpack') fixBackpack(ts);
  if (isPrivate && ts.account && rust.account) {
    if (values.venue === 'cryptocom') fixCryptocomAccount(ts.account);
    KNOWN_TS_FIXES(ts.account, rust.account);
    fixHistoricalFills(ts.commands);
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(ts.account[field]);
      const right = JSON.stringify(rust.account[field]);
      if (!equal(ts.account[field], rust.account[field])) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
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
  // KNOWN_TS_FIXES: the existing Bybit Rust port also subscribes to execution
  // and wallet for execution IDs and live balances. TS subscribes only to
  // order/position. Require those two exact extra topics on every private
  // connection rather than dropping them from the comparison.
  if (values.venue === 'bybit' && ts.account) {
    for (const row of [...ts.subscriptions]) {
      const [url, connection, op, topic] = JSON.parse(row);
      if (new URL(url).pathname === '/v5/private' && op === 'subscribe' && topic === 'order') {
        for (const extra of ['execution', 'wallet']) ts.subscriptions.push(JSON.stringify([url, connection, op, extra]));
      }
    }
    ts.subscriptions.sort();
  }
  for (const field of ['outbound', 'subscriptions', 'candles', 'bookEvents', 'wsRequests']) {
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
if (values.venue.startsWith('okx')) console.log(`(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`);

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
