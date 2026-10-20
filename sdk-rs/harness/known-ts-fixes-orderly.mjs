// Independent private Orderly replay corrections. Every value comes from the
// documented tape; unexpected TS behavior fails instead of being normalized.
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { parseRequestJson } from './request-json.mjs';
import { requestKey } from './requests.mjs';

export const ORDERLY_TS_FIXES = [
  'keyless full-depth snapshots use documented WS requests rather than signed REST',
  'broker-suffixed symbols retain their native identity and USDC quote',
  'funding uses per-market topics on shards below the 100-topic socket limit',
  'native TIF and exact split quantities replace ignored or drifting legacy fields',
  'protective cancellation confirms old parents and preserves each sibling once',
  'failed and stale reconciliation reads preserve newer account state',
  'duplicate execution reports emit exactly one incremental fill',
  'orders retain native client IDs, time-in-force, trigger source and zero market price',
  'root cancellation terminates all children and later snapshots prune terminal rows',
  'symbol settings use the selected margin mode instead of the last position row',
  'positional placement returns the documented root ID rather than legacy child IDs',
  'symbol leverage reads and four history/public-trade queries replace inherited stubs',
  'open-order reads request the documented explicit page and maximum page size',
  'zero-open-price ticker percentages remain finite instead of serializing NaN as null',
];
const parse = parseRequestJson;
const native = (symbol) => symbol.replace(/^(.+)USDC(_[A-Za-z0-9_-]+)?$/, (_, base, suffix = '') => `PERP_${base}_USDC${suffix}`);
const symbol = (value) => /^PERP_.+_USDC(?:_[A-Za-z0-9_-]+)?$/.test(value ?? '') ? value.replace(/^PERP_(.+)_USDC(.*)$/, '$1USDC$2') : undefined;
const side = (value) => ({ BUY: 'buy', SELL: 'sell' })[value];
const terminal = (status) => ['FILLED', 'CANCELLED', 'CANCELED', 'REJECTED', 'EXPIRED'].includes(status);
const orderStatus = (value) => value === 'FILLED' ? 'closed' : terminal(value) ? 'canceled' : 'open';
const type = (value) => ({ LIMIT: 'limit', MARKET: 'market', POST_ONLY: 'limit', IOC: 'limit', FOK: 'limit',
  STOP_LOSS: 'stop_market', TAKE_PROFIT: 'take_profit_market', TRAILING_STOP: 'trailing_stop_market' })[value];
const tif = (value) => ({ LIMIT: 'GoodTillCancel', POST_ONLY: 'PostOnly', IOC: 'ImmediateOrCancel', FOK: 'FillOrKill' })[value];
const success = (record) => record.type === 'http' && record.status >= 200 && record.status < 300 && parse(record.body).success === true;
const path = (record) => new URL(record.url).pathname;
const data = (record) => parse(record.body).data;
const number = (value) => {
  const n = Number(value);
  assert.ok(Number.isFinite(n), `invalid documented numeric value ${value}`);
  return n;
};
const id = (value) => {
  const text = String(value);
  assert.match(text, /^[1-9]\d*$/, 'missing exact native ID in oracle');
  return text;
};
const change = (object, key, legacy, expected, label) => {
  if (isDeepStrictEqual(object[key], expected)) return;
  assert.deepEqual(object[key], legacy, `unexpected TS ${label}`);
  object[key] = expected;
};
const missing = (object, key, expected, label) => {
  if (isDeepStrictEqual(object[key], expected)) return;
  assert.ok(object[key] === null || object[key] === undefined, `unexpected TS ${label}`);
  object[key] = expected;
};
const byTime = (rows, timeKey) => rows.sort((a, b) => number(b[timeKey]) - number(a[timeKey]) ||
  (String(a.id ?? a.tranId) < String(b.id ?? b.tranId) ? 1 : String(a.id ?? a.tranId) > String(b.id ?? b.tranId) ? -1 : 0));
const accepted = (row, command, time) => (!command.symbol || symbol(row.symbol) === command.symbol) &&
  (command.from === undefined || time >= command.from) && (command.to === undefined || time <= command.to);

function readPages(records, command, commandIndex, route, requests) {
  const out = [];
  const seen = new Set();
  for (let page = 1; page <= 50; page++) {
    let choices = records.filter((record) => record.type === 'http' && record.method === 'GET' &&
      path(record) === route && (record.afterCommand ?? 0) <= commandIndex).filter((record) => {
      const query = new URL(record.url).searchParams;
      if (/^\/v1\/order\/\d+\/trades$/.test(route)) return true;
      return (query.get('symbol') ?? undefined) === (command.symbol ? native(command.symbol) : undefined) &&
        (query.get('start_t') ?? undefined) === (command.from === undefined ? undefined : String(command.from)) &&
        (query.get('end_t') ?? undefined) === (command.to === undefined ? undefined : String(command.to)) &&
        Number(query.get('page') ?? 1) === page && !query.has('status');
    });
    assert.ok(choices.length, `missing documented oracle page ${route} ${page}`);
    const stage = Math.max(...choices.map((r) => r.afterCommand ?? 0));
    choices = choices.filter((r) => (r.afterCommand ?? 0) === stage);
    const preferred = choices.find((r) => new URL(r.url).searchParams.get('size') === '500') ?? choices[0];
    assert.ok(success(preferred), `failed documented oracle page ${route}`);
    requests.add(requestKey('GET', preferred.url));
    const response = data(preferred);
    assert.ok(Array.isArray(response.rows), `missing documented oracle rows ${route}`);
    assert.ok(response.rows.length <= 500, 'oversized oracle history page');
    let fresh = 0;
    for (const row of response.rows) {
      const key = row.id ?? row.order_id ?? row.algo_order_id ?? JSON.stringify(row);
      if (!seen.has(String(key))) { seen.add(String(key)); out.push(row); fresh++; }
    }
    const more = response.meta?.total === undefined ? response.rows.length === 500 : seen.size < response.meta.total;
    if (!more) return out;
    assert.ok(fresh > 0, 'repeated documented oracle page');
  }
  throw new Error('documented oracle history exceeds page budget');
}

function queryExpected(records, command, index, requests) {
  const limit = Math.min(command.limit ?? (command.op === 'fetchIncome' ? 1000 : 100), 1000);
  if (command.op === 'fetchFills') {
    const route = command.orderId ? `/v1/order/${id(command.orderId)}/trades` : '/v1/trades';
    const rows = readPages(records, command, index, route, requests).filter((r) => accepted(r, command, number(r.executed_timestamp)) &&
      (!command.orderId || id(r.order_id) === command.orderId)).map((r) => ({
      id: id(r.id), timestamp: number(r.executed_timestamp) / 1000, symbol: symbol(r.symbol), orderId: id(r.order_id),
      price: number(r.executed_price), qty: number(r.executed_quantity), side: side(r.side), commission: number(r.fee),
      commissionAsset: r.fee_asset, ...(r.realized_pnl == null ? {} : { realizedPnl: number(r.realized_pnl) }),
      ...(r.is_maker == null ? {} : { maker: r.is_maker === true || r.is_maker === 1 }),
    }));
    return byTime(rows, 'timestamp').slice(0, limit);
  }
  if (command.op === 'fetchOrderHistory') {
    const rows = [];
    const walk = (row, parent, algo, depth = 0) => {
      assert.ok(depth <= 8, 'oracle algo tree too deep');
      const raw = { ...parent, ...row };
      const status = row.algo_status ?? row.status ?? row.root_algo_order_status ?? parent.algo_status ?? parent.status;
      const container = ['BRACKET', 'POSITIONAL_TP_SL', 'TP_SL'].includes(row.algo_type);
      if ((!container || row.algo_type === 'BRACKET') && terminal(status)) {
        const stamp = number(raw.created_time);
        if (accepted(raw, command, stamp)) {
          const reduceOnly = raw.reduce_only ?? ['STOP_LOSS', 'TAKE_PROFIT', 'TRAILING_STOP', 'POSITIONAL_TP_SL'].includes(row.algo_type);
          const kind = row.algo_type === 'STOP' ? (reduceOnly ? 'stop_market' : 'conditional') : type(row.algo_type === 'BRACKET' || !algo ? row.type : row.algo_type);
          assert.ok(kind, 'unsupported documented oracle order kind');
          const price = row.trigger_price ?? row.price;
          rows.push({ id: id(algo ? row.algo_order_id : row.order_id), symbol: symbol(raw.symbol), side: side(raw.side),
            type: kind, status: orderStatus(status), ...(number(price ?? 0) > 0 ? { price: number(price) } : {}),
            ...(number(row.average_executed_price ?? 0) > 0 ? { averagePrice: number(row.average_executed_price) } : {}),
            amount: number(row.quantity), filled: number(row.total_executed_quantity ?? row.executed ?? 0),
            reduceOnly, createdTime: stamp, ...(raw.updated_time == null ? {} : { updatedTime: number(raw.updated_time) }) });
        }
      }
      for (const child of row.child_orders ?? []) walk(child, raw, algo, depth + 1);
    };
    for (const [route, algo] of [['/v1/orders', false], ['/v1/algo/orders', true]])
      for (const row of readPages(records, command, index, route, requests)) walk(row, {}, algo);
    return byTime(rows, 'createdTime').slice(0, limit);
  }
  if (command.op === 'fetchIncome') {
    assert.ok(command.incomeType == null || ['FUNDING', 'FUNDING_FEE'].includes(command.incomeType.toUpperCase()), 'unsupported oracle income category');
    const unique = new Set();
    const rows = readPages(records, command, index, '/v1/funding_fee/history', requests).filter((r) => r.status === 'Settled' && accepted(r, command, number(r.created_time))).flatMap((r) => {
      assert.ok(['Pay', 'Receive'].includes(r.payment_type), 'unknown funding payment direction');
      const key = `${r.symbol}:${r.margin_mode ?? 'CROSS'}:${number(r.created_time)}:${r.payment_type}:${number(r.funding_fee)}:${number(r.funding_rate)}:${number(r.mark_price)}`;
      if (unique.has(key)) return [];
      unique.add(key);
      return [{ symbol: symbol(r.symbol), incomeType: 'FUNDING_FEE', income: Math.abs(number(r.funding_fee)) * (r.payment_type === 'Pay' ? -1 : 1),
        asset: 'USDC', timestamp: number(r.created_time) / 1000, tranId: `orderly-funding:${key}`, category: 'funding' }];
    });
    return byTime(rows, 'timestamp').slice(0, limit);
  }
  const record = records.find((r) => r.type === 'http' && r.method === 'GET' && path(r) === '/v1/public/market_trades' &&
    new URL(r.url).searchParams.get('symbol') === native(command.symbol) && Number(new URL(r.url).searchParams.get('limit')) === Math.min(limit, 500));
  assert.ok(record && success(record), 'missing documented public-trade oracle');
  requests.add(requestKey('GET', record.url));
  const occurrences = new Map();
  const rows = data(record).rows.map((r) => {
    const price = number(r.executed_price), qty = number(r.executed_quantity), timestamp = number(r.executed_timestamp);
    const base = `orderly-trade:${symbol(r.symbol)}:${timestamp}:${side(r.side)}:${price}:${qty}`;
    const occurrence = occurrences.get(base) ?? 0;
    occurrences.set(base, occurrence + 1);
    return { id: `${base}:${occurrence}`, symbol: symbol(r.symbol), price, qty, quoteQty: price * qty, timestamp: timestamp / 1000, side: side(r.side) };
  });
  return byTime(rows, 'timestamp').slice(0, Math.min(limit, 500));
}

function correctFills(account, records) {
  const legacy = [], expected = [], seen = new Set(), cumulative = new Map();
  for (const record of records) {
    if (record.type !== 'ws') continue;
    const event = parse(record.text);
    if (event.topic !== 'executionreport') continue;
    const r = event.data;
    if (!['PARTIAL_FILLED', 'PARTIALLY_FILLED', 'FILLED'].includes(r.status)) continue;
    const orderId = r.orderId ?? r.order_id;
    const fill = { id: orderId, symbol: symbol(r.symbol), side: side(r.side), amount: number(r.executedQuantity), price: number(r.executedPrice) };
    legacy.push(fill);
    const total = number(r.totalExecutedQuantity ?? r.total_executed_quantity ?? r.executedQuantity);
    const key = r.tradeId == null ? `${orderId}:${r.timestamp ?? event.ts}:${total}` : `${orderId}:${r.tradeId}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const prior = cumulative.get(String(orderId)) ?? 0;
    const amount = Math.max(total - prior, 0);
    cumulative.set(String(orderId), Math.max(total, prior));
    if (amount > 0) expected.push({ ...fill, id: id(orderId), amount });
  }
  if (legacy.length) change(account, 'fills', legacy, expected, 'duplicate execution fills');
}

function pairFor(records, order) {
  const stop = order.type === 'stop_market' ? order.price : order.stopLoss;
  const take = order.type === 'take_profit_market' ? order.price : order.takeProfit;
  const candidates = records.filter((r) => r.type === 'http' && r.method === 'POST' && path(r) === '/v1/algo/order' && r.requestBody).filter((r) => {
    const body = parse(r.requestBody);
    return body.symbol === native(order.symbol) &&
      (stop === undefined || body.child_orders.some((c) => c.algo_type === 'STOP_LOSS' && number(c.trigger_price) === stop)) &&
      (take === undefined || body.child_orders.some((c) => c.algo_type === 'TAKE_PROFIT' && number(c.trigger_price) === take));
  });
  assert.equal(candidates.length, 1, 'ambiguous recorded positional placement');
  assert.ok(success(candidates[0]), 'failed positional oracle response');
  return candidates[0];
}

function correctCommands(ts, records) {
  const commands = records.filter((r) => r.type === 'command');
  assert.equal(ts.commands.length, commands.length, 'missing TS command result');
  let lastPairCommand = 0;
  const requiredReads = new Set();
  for (const [index, command] of commands.entries()) {
    const result = ts.commands[index];
    if (['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(command.op)) {
      const legacy = command.op === 'fetchPublicTrades' ? { error: 'Not implemented' } : { ok: [] };
      const queryReads = new Set();
      const expected = { ok: queryExpected(records, command, index + 1, queryReads) };
      if (!isDeepStrictEqual(result, expected)) {
        assert.deepEqual(result, legacy, `unexpected TS ${command.op} fallback`);
        ts.commands[index] = expected;
      }
      if (isDeepStrictEqual(result, legacy)) for (const key of queryReads) requiredReads.add(key);
    } else if (command.op === 'fetchLeverage') {
      const response = records.find((r) => r.type === 'http' && r.method === 'GET' && path(r) === '/v1/client/leverage' && new URL(r.url).searchParams.get('symbol') === native(command.symbol));
      assert.ok(response && success(response), 'missing symbol leverage oracle');
      const leverage = number(data(response).max_leverage ?? data(response).leverage);
      const legacy = isDeepStrictEqual(result, { ok: null });
      change(ts.commands, index, { ok: null }, { ok: { buyLeverage: leverage, sellLeverage: leverage } }, 'symbol leverage fallback');
      if (legacy) requiredReads.add(requestKey('GET', response.url));
    } else if (command.op === 'placeOrders' && command.orders.some((o) => o.stopLoss !== undefined || o.takeProfit !== undefined || ['stop_market', 'take_profit_market'].includes(o.type))) {
      const old = [], fixed = [];
      for (const order of command.orders) {
        if (['limit', 'market'].includes(order.type)) {
          const candidates = records.filter((r) => r.type === 'http' && r.method === 'POST' && path(r) === '/v1/order' && r.requestBody).filter((r) => {
            const body = parse(r.requestBody);
            return body.symbol === native(order.symbol) && body.order_type === order.type.toUpperCase() &&
              body.side === order.side.toUpperCase() && number(body.order_quantity) === order.amount;
          });
          assert.equal(candidates.length, 1, 'ambiguous recorded bracket entry');
          assert.ok(success(candidates[0]), 'failed bracket entry oracle');
          old.push(id(data(candidates[0]).order_id)); fixed.push(id(data(candidates[0]).order_id));
        }
        const response = data(pairFor(records, order));
        assert.ok(response.order_id && Array.isArray(response.rows), 'root correction needs explicit root and legacy child IDs');
        old.push(...response.rows.map((r) => id(r.orderId)));
        fixed.push(id(response.order_id));
      }
      change(ts.commands, index, { ok: old }, { ok: fixed }, 'documented positional root IDs');
      lastPairCommand = index + 1;
    }
  }
  if (Array.isArray(ts.requests)) ts.requests = [...new Set([...ts.requests, ...requiredReads])].sort();
  return lastPairCommand;
}

function correctOrders(account, records, lastPairCommand) {
  const metadata = new Map(), roots = new Map(), finals = new Map();
  const observe = (row, parent) => {
    if (row.child_orders?.length) { for (const child of row.child_orders) observe(child, row); return; }
    const rawId = row.order_id ?? row.orderId ?? row.algo_order_id ?? row.algoOrderId;
    if (rawId == null) return;
    const key = id(rawId);
    metadata.set(key, { ...metadata.get(key), ...parent, ...row });
    const root = row.root_algo_order_id ?? row.rootAlgoOrderId ?? parent?.root_algo_order_id ?? parent?.rootAlgoOrderId;
    if (root != null) roots.set(key, String(root));
    if (terminal(row.status ?? row.algo_status ?? row.algoStatus)) finals.set(key, row.status ?? row.algo_status ?? row.algoStatus);
  };
  for (const r of records) {
    if (r.type === 'http' && r.method === 'GET' && success(r) && ['/v1/orders', '/v1/algo/orders'].includes(path(r)) && new URL(r.url).searchParams.get('status') === 'INCOMPLETE')
      for (const row of data(r).rows) observe(row);
    if (r.type !== 'ws') continue;
    const event = parse(r.text);
    if (event.topic === 'executionreport') observe(event.data);
    if (event.topic === 'algoexecutionreportv2') {
      for (const row of Array.isArray(event.data) ? event.data : [event.data]) {
        const root = row.root_algo_order_id ?? row.rootAlgoOrderId ?? row.algo_order_id ?? row.algoOrderId;
        const status = row.algo_status ?? row.algoStatus;
        if (terminal(status) && !row.child_orders?.length) {
          for (const [child, parent] of roots) if (parent === String(root)) finals.set(child, status);
        } else observe(row);
      }
    }
  }
  const retained = new Set();
  if (lastPairCommand) {
    for (const route of ['/v1/orders', '/v1/algo/orders']) {
      const candidates = records.filter((r) => r.type === 'http' && r.method === 'GET' && path(r) === route &&
        new URL(r.url).searchParams.get('status') === 'INCOMPLETE' && (r.afterCommand ?? 0) <= lastPairCommand);
      const stage = Math.max(...candidates.map((r) => r.afterCommand ?? 0));
      const response = candidates.find((r) => (r.afterCommand ?? 0) === stage);
      assert.ok(response && success(response), 'missing post-pair reconciliation oracle');
      for (const row of data(response).rows) {
        if (row.child_orders?.length) for (const child of row.child_orders) retained.add(String(child.algo_order_id));
        else retained.add(String(row.order_id ?? row.algo_order_id));
      }
    }
  }
  account.orders = account.orders.filter((order) => {
    if (lastPairCommand && finals.has(order.id) && !retained.has(order.id)) {
      assert.ok(['open', 'closed', 'canceled'].includes(order.status), 'unexpected TS terminal row state');
      return false;
    }
    const raw = metadata.get(order.id);
    if (!raw) return true;
    const rawType = raw.type ?? raw.order_type;
    if (['POST_ONLY', 'IOC', 'FOK'].includes(rawType)) missing(order, 'type', 'limit', 'native limit variant');
    const caller = raw.clientOrderId ?? raw.client_order_id;
    if (caller != null && caller !== '') change(order, 'clientOrderId', null, String(caller), 'missing client ID');
    if (order.type === 'limit' && tif(rawType)) missing(order, 'timeInForce', tif(rawType), 'missing native TIF');
    if (['stop_market', 'take_profit_market', 'trailing_stop_market'].includes(order.type)) change(order, 'trigger', null, 'mark_price', 'missing MarkPrice trigger');
    if (rawType === 'MARKET' && number(raw.price ?? raw.order_price ?? 0) === 0) missing(order, 'price', 0, 'missing zero market price');
    if (finals.has(order.id)) change(order, 'status', 'open', orderStatus(finals.get(order.id)), 'ignored parent terminal status');
    return true;
  });
}

function correctSettings(account, records) {
  const defaults = new Map(), initialDefaults = new Map(), leverages = new Map(), positions = new Map(), markets = new Set();
  let fallback = 20;
  for (const record of records) {
    if (record.type === 'http' && success(record)) {
      const route = path(record), response = data(record);
      if (record.method === 'GET') {
        if (route === '/v1/public/info') for (const row of response.rows) markets.add(symbol(row.symbol));
        if (route === '/v1/client/info') fallback = number(response.max_leverage);
        if (route === '/v1/client/margin_modes') for (const row of response.rows) {
          defaults.set(symbol(row.symbol), row.default_margin_mode.toLowerCase());
          if (!initialDefaults.has(symbol(row.symbol))) initialDefaults.set(symbol(row.symbol), row.default_margin_mode.toLowerCase());
        }
        if (route === '/v1/positions') for (const row of response.rows) positions.set(`${row.symbol}:${row.margin_mode}`, row);
      } else if (record.requestBody) {
        const body = parse(record.requestBody);
        if (route === '/v1/client/margin_mode') defaults.set(symbol(body.symbol), body.default_margin_mode.toLowerCase());
        if (route === '/v1/client/leverages' && body.symbol) leverages.set(`${body.symbol}:${body.margin_mode}`, number(body.leverage));
      }
    } else if (record.type === 'ws') {
      const event = parse(record.text);
      if (event.topic === 'position') for (const row of event.data.positions ?? []) {
        const mode = row.marginMode ?? row.margin_mode;
        positions.set(`${row.symbol}:${mode}`, { ...row, margin_mode: mode });
      }
    }
  }
  for (const row of positions.values()) {
    const key = `${row.symbol}:${row.margin_mode}`;
    if (!leverages.has(key) && row.leverage != null) leverages.set(key, number(row.leverage));
  }
  const legacy = new Map();
  for (const row of positions.values()) legacy.set(symbol(row.symbol), { symbol: symbol(row.symbol), marginType: row.margin_mode.toLowerCase(), leverage: number(row.leverage ?? fallback) });
  const expected = [...markets].map((s) => ({ symbol: s, marginType: defaults.get(s) ?? 'cross', leverage: leverages.get(`${native(s)}:${(defaults.get(s) ?? 'cross').toUpperCase()}`) ?? fallback }));
  for (const row of account.settings) {
    const wanted = expected.find((r) => r.symbol === row.symbol);
    if (!wanted || isDeepStrictEqual(row, wanted)) continue;
    const old = legacy.get(row.symbol) ?? { symbol: row.symbol, marginType: initialDefaults.get(row.symbol) ?? 'cross', leverage: fallback };
    assert.deepEqual(row, old, 'unexpected TS last-position setting');
    Object.assign(row, wanted);
  }
}

function correctOpenOrderRequests(ts, records) {
  if (!Array.isArray(ts.requests)) return;
  const replacements = new Map();
  for (const record of records) {
    if (record.type !== 'http' || record.method !== 'GET' || !success(record) ||
        !['/v1/orders', '/v1/algo/orders'].includes(path(record))) continue;
    const url = new URL(record.url);
    if (url.searchParams.get('status') !== 'INCOMPLETE' || url.searchParams.get('page') !== '1' || url.searchParams.get('size') !== '500') continue;
    const paged = requestKey('GET', url);
    url.searchParams.delete('page');
    url.searchParams.delete('size');
    replacements.set(requestKey('GET', url), paged);
  }
  // Replace only the precise legacy unpaged reads; writes and unexpected reads
  // remain visible to Tape and the gate's exact request comparison.
  ts.requests = [...new Set(ts.requests.map((key) => replacements.get(key) ?? key))].sort();
}

function correctZeroOpenPercentages(ts, records) {
  const zeroOpen = new Set();
  for (const record of records) {
    if (record.type !== 'http' || record.method !== 'GET' || path(record) !== '/v1/public/futures' || !success(record)) continue;
    for (const row of data(record).rows) {
      if (row['24h_open'] === 0 && row['24h_close'] === 0) zeroOpen.add(symbol(row.symbol));
      else zeroOpen.delete(symbol(row.symbol));
    }
  }
  for (const ticker of ts.tickers ?? [])
    if (zeroOpen.has(ticker.symbol) && ticker.last === 0 && ticker.percentage === null) ticker.percentage = 0;
}

export function fixOrderly(ts, records) {
  assert.ok(Array.isArray(records), 'Orderly corrections require input records');
  correctZeroOpenPercentages(ts, records);
  const privateTape = records.find((r) => r.type === 'meta')?.private ?? records.some((r) => r.url?.includes('/private/stream/'));
  if (!privateTape) return;
  correctOpenOrderRequests(ts, records);
  const lastPair = correctCommands(ts, records);
  if (!ts.account) return;
  correctFills(ts.account, records);
  correctOrders(ts.account, records, lastPair);
  correctSettings(ts.account, records);
}
