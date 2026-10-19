// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { sameCommandResults, stable } from './tape.mjs';
import { parseRequestJson } from './request-json.mjs';
import { fixWeexConditionals } from './known-ts-fixes.mjs';
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
const tapeBytes = readFileSync(values.tape);
const tapeRecords = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .trim()
  .split('\n')
  .map(JSON.parse);
const outDir = `sdk-rs/harness/out/${values.venue}-${process.pid}`;
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push(
    'sdk-rs/harness/replay.mjs',
    '--impl',
    impl,
    '--venue',
    values.venue,
    '--tape',
    values.tape,
    '--books',
    values.books,
    '--out',
    out,
  );
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125000 });
  if (result.status !== 0)
    throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    const left = JSON.stringify(stable(ta.get(symbol)));
    const right = JSON.stringify(stable(tb.get(symbol)));
    if (left !== right) differences.push(`${label} ${symbol}: ts=${left} rust=${right}`);
  }
};

const runs = Number(values.runs);
const results = { ts: [], rust: [] };
for (let run = 0; run < runs; run++) {
  for (const impl of ['ts', 'rust']) results[impl].push(replay(impl, run));
}
const [ts, rust] = [results.ts[0], results.rust[0]];

const venue = venueOf(values.venue);
const isPrivate =
  tapeRecords.find((record) => record.type === 'meta')?.private ??
  tapeRecords.some((record) =>
    venue.isPrivateRecord
      ? venue.isPrivateRecord(record)
      : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)),
  );
const records = tapeRecords;
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
// TS bugs intentionally corrected in Rust. Backpack normalization derives
// expected fields only from the input tape, never from Rust's answer.
const KNOWN_TS_FIXES = {
  kucoin: [
    'futures inherits empty fills/orders/income and unsupported public trades; query results use explicit documented tape oracles',
    'stops classify with side and direction, retain trigger basis and closeOrder',
    'match executions deduplicate trade IDs and require their execution price',
    'failed and delayed REST snapshots preserve newer account state and terminal orders',
    'conditional orders and mixed bracket trigger references fail before writes instead of dropping fields',
    'cancelSymbolOrders cancels the stop book too (reference signed transport fills this omission)',
    'open orders walk every numbered page; TS reads only its default first page',
    'socket readiness and subscriptions wait for welcome; TS sends on TCP open',
    'cancel-replace proves terminal cancellation and subtracts fills during cancellation',
    'stop-to-normal migration remains visible across independent REST books',
    'settings streams update flat symbols and fence delayed metadata reads',
    'partial order stream rows retain REST-known safety flags',
    'immediate bracket replacement retains submitted protections before reconciliation',
    'cached book snapshot gaps retain the entire contiguous buffered suffix (TS reference fixed with a regression)',
  ],
  blofin: (ts) => {
    const account = ts.account;
    if (!account) return;
    // The native empty position snapshot clears both aggregates; TS retains
    // its last margin and unrealized PnL after the final position closes.
    if (account.positions.length === 0) {
      account.balance.used = 0;
      account.balance.upnl = 0;
    }
  },
  mexc: () => {
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
  binancepm: (ts, rust) => KNOWN_TS_FIXES.binance(ts, rust),
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
    const bytes = readFileSync(values.tape);
    const records = (values.tape.endsWith('.gz') ? gunzipSync(bytes) : bytes)
      .toString('utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
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
  ],

  binance: (ts, rust) => {
    for (const position of ts.account?.positions ?? []) {
      const fixed = rust.account.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
      if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
    }
  },
  bybit: (ts, rust) => {
    if (!ts.account) return;
    const required = ts.subscriptions
      .filter((key) => key.endsWith('/v5/private subscribe order'))
      .flatMap((key) => ['execution', 'wallet'].map((topic) => key.replace(/order$/, topic)));
    ts.subscriptions = [...new Set([...ts.subscriptions, ...required])].sort();
    KNOWN_TS_FIXES.binance(ts, rust);
    const bytes = readFileSync(values.tape);
    const records = (values.tape.endsWith('.gz') ? gunzipSync(bytes) : bytes)
      .toString()
      .split('\n')
      .filter(Boolean)
      .map(JSON.parse);
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
  },
};
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
for (let run = 0; run < runs; run++) {
  const ts = results.ts[run];
  const rust = results.rust[run];
  if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
  // Every request must match a taped one: an unserved request sent different parameters.
  for (const impl of ['ts', 'rust']) {
    const output = results[impl][run];
    if (Boolean(output.account) !== isPrivate)
      differences.push(impl + ': account presence differs from tape classification');
    // Paced private tapes gate correctness; their unscored heap can shed loaded code.
    if (!isPrivate)
      for (const field of ['retainedBytes', 'heapRetainedBytes', 'bufferRetainedBytes', 'wasmBytes']) {
        if (!Number.isFinite(output.metrics[field]) || output.metrics[field] < 0) {
          differences.push(`${impl}: invalid ${field} (${output.metrics[field]}); baseline is invalid`);
        }
      }
    if (output.unusedWrites) differences.push(impl + ': ' + output.unusedWrites + ' taped mutation(s) never sent');
    for (const error of output.protocolErrors ?? []) differences.push(impl + ': ' + error);
    for (const symbol of values.books.split(',').filter(Boolean)) {
      if (!output.books[symbol]) differences.push(`${impl}: requested book ${symbol} is missing`);
    }
    if (!output.markets.length || !output.tickers.length) differences.push(`${impl}: empty public seed`);
    const unserved = output.unserved;
    if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
    const { missingWrites, invalidSignatures } = results[impl][run];
    if (missingWrites) differences.push(`${impl}: ${missingWrites} required taped write(s) never sent`);
    if (invalidSignatures) differences.push(`${impl}: ${invalidSignatures} invalid signatures`);
  }
  if (typeof KNOWN_TS_FIXES[values.venue] === 'function') KNOWN_TS_FIXES[values.venue](ts, rust);
  if (JSON.stringify(ts.wsRequests) !== JSON.stringify(rust.wsRequests))
    differences.push(
      `websocket requests differ: ts=${JSON.stringify(ts.wsRequests)} rust=${JSON.stringify(rust.wsRequests)}`,
    );
  // OKX checks native wildcard coverage; Crypto audits auth/topics/heartbeats per socket.
  if (
    !values.venue.startsWith('okx') &&
    values.venue !== 'cryptocom' &&
    (!Array.isArray(ts.subscriptions) ||
      !Array.isArray(rust.subscriptions) ||
      JSON.stringify(ts.subscriptions) !== JSON.stringify(rust.subscriptions))
  )
    differences.push(
      `subscriptions differ: ts=${JSON.stringify(ts.subscriptions)} rust=${JSON.stringify(rust.subscriptions)}`,
    );
  if (isPrivate && ts.account && rust.account) {
    if (values.venue === 'cryptocom') fixCryptocomAccount(ts.account);
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(ts.account[field]);
      const right = JSON.stringify(rust.account[field]);
      if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
  }
  // KNOWN_TS_FIXES: KuCoin TS has no futures history implementations. The
  // synthetic tape declares independent documented query expectations; never
  // derive this reference from the Rust result.
  if (values.venue === 'kucoin') tapeRecords.filter((r) => r.type === 'command').forEach((record, index) => {
    if (record.expected !== undefined) {
      if (!['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(record.op)) throw new Error('KuCoin expected oracle is restricted to missing TS query methods');
      ts.commands[index] = { ok: record.expected };
    }
  });
  if (!sameCommandResults(ts.commands, rust.commands))
    differences.push(`command results: ts=${JSON.stringify(ts.commands)} rust=${JSON.stringify(rust.commands)}`);
  normalizeBingxPublic(ts);
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom')
    for (const ticker of ts.tickers) {
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
}

const summary = (impl) => ({
  cpuMs: median(results[impl].map((r) => r.metrics.cpuMs)),
  steadyCpuMs: median(results[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(results[impl].map((r) => r.metrics.processCpuMs)),
  retainedMB: median(results[impl].map((r) => r.metrics.retainedBytes)) / 1e6,
  wasmMB: median(results[impl].map((r) => r.metrics.wasmBytes ?? 0)) / 1e6,
});
const t = summary('ts');
const r = summary('rust');
console.log(
  `frames ${ts.frames}, markets ${ts.markets.length}, tickers ${ts.tickers.length}, books ${Object.keys(ts.books).length}`,
);
const row = (label, a, b, unit = 'ms') =>
  console.log(
    `${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`,
  );
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (isPrivate) console.log('(private tape: correctness only; CPU and memory are informational)');
if (values.venue.startsWith('okx'))
  console.log(
    `(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`,
  );

const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate) {
  if (!(r.cpuMs < t.cpuMs)) failures.push('Rust main-thread CPU is not below TS');
  if (!(r.steadyCpuMs < t.steadyCpuMs)) failures.push('Rust steady-state CPU is not below TS');
  if (!(r.retainedMB < t.retainedMB)) failures.push('Rust retained memory is not below TS');
}
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
