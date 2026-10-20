// Independent documented-input oracle. No native replay result enters these repairs.
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { requestKey } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';

export const COINBASE_TS_FIXES = [
  'keyless coinbasespot discovers active spot products rather than the retired public INTX profile',
  'REST product discovery uses explicit bounded pagination and validates native product families',
  'Advanced Trade online and legacy TRADING markets are active only when both disabled flags are false',
  'every socket validates connection-wide sequence numbers and recovers missing protocol payloads',
  'user cumulative quantities produce one incremental fill at its actual incremental execution price',
  'native client IDs and ordinary order prices survive user-stream reconciliation',
  'caller IDs obey the documented 128-character limit and nonfinite optional numbers fail before JSON can discard constraints',
  'spot currency totals include held assets instead of reporting available assets twice',
  'CFM balances retain the API unrealized PnL rather than hardcoding zero',
  'spot settings use leverage one and unsupported account mutations fail explicitly',
  'cancel outcomes are checked per ID and INTX replacement waits for authoritative zero-fill cancellation',
  'typed fills, terminal order history and public trades replace inherited query stubs; public maker sides become taker sides',
  'unavailable income and exact client-ID lookup fail explicitly instead of claiming an empty result',
  'daily candles seed the full day and backfill reconnect gaps before replacing cumulative five-minute buckets',
  'spot quote-sized user orders normalize quote leaves separately from base fills using authoritative sizing provenance',
];
const parse = parseRequestJson;
const route = (record) => new URL(record.url).pathname;
const number = (value) => {
  const n = Number(value);
  assert.ok(Number.isFinite(n), 'invalid Coinbase oracle number');
  return n;
};
const successful = (record) => record.type === 'http' && record.status >= 200 && record.status < 300;
const status = (s) =>
  s === 'FILLED' ? 'closed' : ['CANCELLED', 'EXPIRED', 'FAILED'].includes(s) ? 'canceled' : 'open';
const side = (s) => {
  assert.ok(['BUY', 'SELL'].includes(s), 'invalid Coinbase oracle side');
  return s.toLowerCase();
};
// The comparison snapshot carries numeric wire strings as numbers. Keep this
// legacy-shape assertion compatible without changing the tape-derived price.
const comparisonPrice = (value) => {
  if (typeof value !== 'number' && (typeof value !== 'string' || !value.trim())) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
};
const sameOrChange = (object, key, legacy, expected, label) => {
  if (isDeepStrictEqual(object[key], expected)) return false;
  assert.deepEqual(object[key], legacy, `unexpected TS Coinbase ${label}`);
  object[key] = expected;
  return true;
};
function products(records) {
  const out = new Map();
  for (const r of records)
    if (
      successful(r) &&
      r.method === 'GET' &&
      ['/api/v3/brokerage/products', '/api/v3/brokerage/market/products'].includes(route(r))
    )
      for (const row of parse(r.body).products ?? []) out.set(row.product_id, row);
  return out;
}
const scale = (market) =>
  market?.future_product_details?.contract_expiry_type === 'EXPIRING'
    ? number(market.future_product_details.contract_size)
    : 1;
function frames(records) {
  return records
    .filter((r) => r.type === 'ws' && r.url.includes('-ws-user.'))
    .map((r) => ({ record: r, message: parse(r.text) }));
}
function correctFills(account, records, markets) {
  const totals = new Map(),
    values = new Map();
  const legacy = [],
    expected = [];
  let complete = false;
  let snapshot = new Map();
  for (const { message } of frames(records)) {
    if (message.channel !== 'user') continue;
    for (const event of message.events ?? []) {
      assert.ok(['snapshot', 'update'].includes(event.type), 'unknown Coinbase user oracle event');
      const rows = event.orders ?? [];
      for (const row of rows) {
        const product = markets.get(row.product_id);
        assert.ok(product, 'missing Coinbase fill market');
        const factor = scale(product),
          quantity = number(row.cumulative_quantity) * factor;
        const average = number(row.avg_price || 0);
        if (event.type === 'snapshot') {
          snapshot.set(row.order_id, { quantity, average });
          continue;
        }
        if (row.status === 'FILLED')
          legacy.push({
            id: row.order_id,
            symbol: row.product_id,
            side: side(row.order_side),
            amount: quantity,
            price: row.avg_price || 'market',
          });
        const previous = totals.get(row.order_id) ?? snapshot.get(row.order_id)?.quantity ?? (complete ? 0 : undefined);
        if (previous !== undefined && quantity > previous) {
          assert.ok(average > 0, 'missing cumulative fill price oracle');
          const oldValue =
            previous === 0 ? 0 : (values.get(row.order_id) ?? snapshot.get(row.order_id)?.average * previous);
          assert.ok(Number.isFinite(oldValue), 'missing previous cumulative fill value oracle');
          const price = (average * quantity - oldValue) / (quantity - previous);
          assert.ok(price > 0 && Number.isFinite(price), 'invalid incremental fill oracle');
          expected.push({
            id: row.order_id,
            symbol: row.product_id,
            side: side(row.order_side),
            amount: quantity - previous,
            price,
          });
        }
        if (previous === undefined || quantity >= previous) {
          totals.set(row.order_id, quantity);
          values.set(row.order_id, quantity * average);
        }
      }
      if (event.type === 'snapshot' && rows.length < 50) {
        for (const [id, row] of snapshot) {
          totals.set(id, Math.max(totals.get(id) ?? 0, row.quantity));
          if (row.average > 0) values.set(id, row.quantity * row.average);
        }
        snapshot = new Map();
        complete = true;
      }
    }
  }
  if (!isDeepStrictEqual(account.fills, expected)) {
    const priceShape = (rows) => rows.map((row) => ({ ...row, price: comparisonPrice(row.price) }));
    assert.deepEqual(priceShape(account.fills), priceShape(legacy), 'unexpected TS Coinbase terminal fill emission');
    account.fills = expected;
  }
}
function correctOrders(account, records, markets) {
  const latest = new Map();
  for (const { message } of frames(records))
    for (const event of message.channel === 'user' ? (message.events ?? []) : [])
      for (const row of event.orders ?? []) latest.set(row.order_id, row);
  for (const row of account.orders ?? []) {
    const raw = latest.get(row.id);
    if (!raw) continue;
    if (raw.client_order_id) {
      const legacy = row.clientOrderId === undefined ? undefined : null;
      sameOrChange(row, 'clientOrderId', legacy, raw.client_order_id, 'native client ID');
    }
    if (row.type === 'limit' && raw.stop_price === '0')
      sameOrChange(row, 'price', 0, number(raw.limit_price), 'zero-stop ordinary limit price');
    if (row.type !== 'limit' || markets.get(raw.product_id)?.product_type !== 'SPOT') continue;
    const proofs = records.filter(
      (r) => successful(r) && r.method === 'GET' && route(r) === `/api/v3/brokerage/orders/historical/${raw.order_id}`,
    );
    for (const proof of proofs) {
      const detail = parse(proof.body).order;
      assert.equal(detail.order_id, raw.order_id, 'Coinbase sizing proof order mismatch');
      assert.equal(detail.product_id, raw.product_id, 'Coinbase sizing proof product mismatch');
      const configs = Object.values(detail.order_configuration ?? {}).filter((c) => c != null);
      assert.equal(configs.length, 1, 'ambiguous Coinbase sizing proof');
      const config = detail.order_configuration.limit_limit_gtc;
      if (!config?.quote_size || config.base_size) continue;
      assert.ok(number(config.quote_size) > 0, 'invalid Coinbase quote sizing proof');
      const price = number(raw.limit_price),
        filled = number(raw.cumulative_quantity),
        leaves = number(raw.leaves_quantity);
      assert.ok(price > 0 && filled >= 0 && leaves >= 0, 'invalid Coinbase quote order economics');
      assert.equal(row.filled, filled, 'unexpected TS Coinbase quote-sized filled quantity');
      sameOrChange(row, 'remaining', leaves, leaves / price, 'quote-sized remaining quantity');
      sameOrChange(row, 'amount', filled + leaves, filled + leaves / price, 'quote-sized order amount');
    }
  }
}
function correctBalances(account, records) {
  const meta = records.find((r) => r.type === 'meta');
  const cfm = records.find(
    (r) => successful(r) && r.method === 'GET' && route(r) === '/api/v3/brokerage/cfm/balance_summary',
  );
  if (meta?.profile === 'cfm' && cfm) {
    const value = number(parse(cfm.body).balance_summary.unrealized_pnl.value);
    sameOrChange(account.balance, 'upnl', 0, value, 'CFM unrealized PnL');
  }
  const breakdown = records.find(
    (r) => successful(r) && r.method === 'GET' && /^\/api\/v3\/brokerage\/portfolios\//.test(route(r)),
  );
  if (!breakdown || !account.balance?.currencies) return;
  for (const asset of parse(breakdown.body).breakdown.spot_positions ?? []) {
    const row = account.balance.currencies[asset.asset];
    assert.ok(row, 'missing Coinbase held currency oracle');
    sameOrChange(
      row,
      'total',
      number(asset.available_to_trade_crypto),
      number(asset.total_balance_crypto),
      'held currency total',
    );
  }
}
function correctSettings(account, markets) {
  for (const row of account.settings ?? []) {
    const market = markets.get(row.symbol);
    if (market?.product_type === 'SPOT') sameOrChange(row, 'leverage', 10, 1, 'spot leverage');
  }
}
const nativeSymbol = (symbol) =>
  symbol && symbol.endsWith('-USDC') && !['USDT-USDC', 'EURC-USDC'].includes(symbol)
    ? symbol.slice(0, -5) + '-USD'
    : symbol;
const productOf = (markets, symbol) => markets.get(symbol) ?? markets.get(nativeSymbol(symbol));
function queryHeads(records, command, path, key, markets) {
  const product = command.symbol && productOf(markets, command.symbol);
  if (command.symbol) assert.ok(product, 'missing Coinbase query product metadata');
  const family = (product) =>
    product.product_type === 'SPOT'
      ? 'spot'
      : product.future_product_details?.contract_expiry_type === 'EXPIRING'
        ? 'cfm'
        : 'intx';
  const profile = records.find((r) => r.type === 'meta')?.profile ?? 'spot';
  const families = command.symbol ? [family(product)] : profile === 'cfm' ? ['cfm', 'spot'] : [profile];
  return families.map((family) => {
    const url = new URL('https://api.coinbase.com' + path);
    const p = url.searchParams;
    p.set('limit', String(key === 'trades' ? (command.limit ?? 100) : Math.min(command.limit ?? 100, 100)));
    if (key === 'trades') return url;
    if (key === 'fills') p.set('sort_by', 'TRADE_TIME');
    if (key === 'orders')
      for (const status of ['FILLED', 'CANCELLED', 'EXPIRED', 'FAILED']) p.append('order_status', status);
    if (command.symbol) p.set('product_ids', nativeSymbol(product.product_id));
    if (command.orderId !== undefined) p.set('order_ids', command.orderId);
    p.set(key === 'fills' ? 'product_types' : 'product_type', family === 'spot' ? 'SPOT' : 'FUTURE');
    if (key === 'orders' && family !== 'spot')
      p.set('contract_expiry_type', family === 'cfm' ? 'EXPIRING' : 'PERPETUAL');
    if (command.from !== undefined)
      p.set(
        key === 'fills' ? 'start_sequence_timestamp' : 'start_date',
        new Date(key === 'fills' ? Math.max(0, command.from - 1) : command.from).toISOString(),
      );
    if (command.to !== undefined)
      p.set(
        key === 'fills' ? 'end_sequence_timestamp' : 'end_date',
        new Date(Math.min(253402300799999, command.to + 1)).toISOString(),
      );
    return url;
  });
}
function readRows(records, command, index, path, key, markets) {
  const expected = queryHeads(records, command, path, key, markets);
  const headKeys = new Set(expected.map((url) => requestKey('GET', url)));
  const candidates = records.filter((r) => {
    if (
      r.type !== 'http' ||
      r.method !== 'GET' ||
      route(r) !== path ||
      (r.afterCommand ?? 0) > index ||
      (r.availableAt ?? r.effectiveAt ?? r.availableAfter ?? 0) > (command.t ?? Infinity)
    )
      return false;
    const url = new URL(r.url);
    url.searchParams.delete('cursor');
    return headKeys.has(requestKey('GET', url));
  });
  const groups = new Map();
  for (const r of candidates) {
    const key = requestKey('GET', r.url);
    const current = groups.get(key);
    if (
      !current ||
      (r.afterCommand ?? 0) > (current.afterCommand ?? 0) ||
      ((r.afterCommand ?? 0) === (current.afterCommand ?? 0) &&
        (r.availableAt ?? r.effectiveAt ?? r.availableAfter ?? 0) >
          (current.availableAt ?? current.effectiveAt ?? current.availableAfter ?? 0))
    )
      groups.set(key, r);
  }
  const heads = expected.map((url) => groups.get(requestKey('GET', url)));
  assert.ok(heads.every(Boolean), 'missing Coinbase documented query response');
  const rows = [],
    reads = [];
  for (const head of heads) {
    let record = head;
    const cursors = new Set(),
      ids = new Set();
    for (let page = 0; ; page++) {
      assert.ok(page < 50, 'Coinbase oracle page limit');
      assert.ok(successful(record), 'failed Coinbase documented query response');
      const body = parse(record.body);
      assert.ok(Array.isArray(body[key]), 'missing Coinbase query array');
      assert.notEqual(body.proof_token_required, true, 'unexpected Coinbase proof token requirement');
      rows.push(...body[key]);
      reads.push(requestKey('GET', record.url));
      for (const row of body[key]) {
        const time = Date.parse(row.trade_time ?? row.created_time ?? row.time);
        if (key === 'orders' && status(row.status) === 'open') continue;
        if ((command.from === undefined || time >= command.from) && (command.to === undefined || time <= command.to))
          ids.add(row.trade_id ?? row.order_id);
      }
      const more =
        key === 'orders'
          ? body.has_next === true
          : key === 'fills' && body[key].length > 0 && typeof body.cursor === 'string' && body.cursor !== '';
      if (!more || ids.size >= (command.limit ?? 100)) break;
      assert.ok(
        typeof body.cursor === 'string' && body.cursor.length > 0 && !cursors.has(body.cursor),
        'Coinbase oracle missing or repeated cursor',
      );
      cursors.add(body.cursor);
      const url = new URL(head.url);
      url.searchParams.set('cursor', body.cursor);
      record = groups.get(requestKey('GET', url));
      assert.ok(record, 'missing Coinbase cursor response oracle');
    }
  }
  return { rows, reads };
}

function query(records, command, index, markets) {
  const root = '/api/v3/brokerage';
  let result;
  if (command.op === 'fetchFills') {
    const { rows, reads } = readRows(records, command, index, root + '/orders/historical/fills', 'fills', markets);
    const seen = new Set();
    result = rows
      .filter((r) => {
        const id = `${r.product_id}:${r.trade_id}`;
        if (seen.has(id)) return false;
        seen.add(id);
        return true;
      })
      .map((r) => {
        assert.equal(r.trade_type, 'FILL');
        const price = number(r.price);
        return {
          id: r.trade_id,
          timestamp: Date.parse(r.trade_time) / 1000,
          symbol: command.symbol ?? r.product_id,
          orderId: r.order_id,
          price,
          qty: r.size_in_quote ? number(r.size) / price / scale(markets.get(r.product_id)) : number(r.size),
          side: side(r.side),
          commission: number(r.commission),
          ...(['MAKER', 'TAKER'].includes(r.liquidity_indicator) ? { maker: r.liquidity_indicator === 'MAKER' } : {}),
        };
      });
    return { value: bounded(result, command, 'timestamp', 1000), reads };
  }
  if (command.op === 'fetchOrderHistory') {
    const { rows, reads } = readRows(records, command, index, root + '/orders/historical/batch', 'orders', markets);
    const seen = new Set();
    result = rows
      .filter((r) => {
        if (seen.has(r.order_id) || status(r.status) === 'open') return false;
        seen.add(r.order_id);
        return true;
      })
      .map((r) => {
        const c = r.order_configuration;
        assert.ok(c, 'missing Coinbase historical configuration');
        const config =
          c.limit_limit_gtc ??
          c.limit_limit_gtd ??
          c.limit_limit_fok ??
          c.sor_limit_ioc ??
          c.market_market_ioc ??
          c.market_market_fok ??
          c.stop_limit_stop_limit_gtc ??
          c.stop_limit_stop_limit_gtd;
        assert.ok(config);
        const kind =
          c.market_market_ioc || c.market_market_fok
            ? 'market'
            : c.stop_limit_stop_limit_gtc || c.stop_limit_stop_limit_gtd
              ? 'stop_market'
              : 'limit';
        const price = number(
          kind === 'stop_market' ? config.stop_price : (config.limit_price ?? r.average_filled_price ?? 0),
        );
        return {
          id: r.order_id,
          symbol: command.symbol ?? r.product_id,
          side: side(r.side ?? r.order_side),
          type: kind,
          status: status(r.status),
          ...(price > 0 ? { price } : {}),
          ...(number(r.average_filled_price ?? 0) > 0 ? { averagePrice: number(r.average_filled_price) } : {}),
          amount: config.base_size ? number(config.base_size) : number(config.quote_size) / price,
          filled: number(r.filled_size),
          reduceOnly: Boolean(r.reduce_only),
          createdTime: Date.parse(r.created_time),
          ...(r.last_fill_time ? { updatedTime: Date.parse(r.last_fill_time) } : {}),
        };
      });
    return { value: bounded(result, command, 'createdTime', 1), reads };
  }
  const { rows, reads } = readRows(
    records,
    command,
    index,
    `${root}/market/products/${nativeSymbol(command.symbol)}/ticker`,
    'trades',
    markets,
  );
  result = rows.map((r) => ({
    id: r.trade_id,
    symbol: command.symbol,
    price: number(r.price),
    qty: number(r.size),
    quoteQty: number(r.price) * number(r.size) * scale(markets.get(r.product_id)),
    timestamp: Date.parse(r.time),
    side: side(r.side) === 'buy' ? 'sell' : 'buy',
  }));
  return { value: bounded(result, command, 'timestamp', 1), reads };
}
function bounded(rows, command, timeKey, multiplier) {
  return rows
    .filter(
      (r) =>
        (command.from === undefined || r[timeKey] * multiplier >= command.from) &&
        (command.to === undefined || r[timeKey] * multiplier <= command.to),
    )
    .sort((a, b) => b[timeKey] - a[timeKey])
    .slice(0, command.limit ?? 100);
}
function correctCommands(ts, records, markets) {
  if (!Array.isArray(ts.commands)) return;
  const commands = records.filter((r) => r.type === 'command');
  assert.equal(ts.commands.length, commands.length, 'missing Coinbase TS command result');
  const reads = new Set();
  for (const [index, c] of commands.entries()) {
    const value = ts.commands[index];
    if (['fetchFills', 'fetchOrderHistory', 'fetchPublicTrades'].includes(c.op)) {
      const mapped = query(records, c, index + 1, markets),
        expected = { ok: mapped.value };
      const legacy = c.op === 'fetchPublicTrades' ? { error: 'Not implemented' } : { ok: [] };
      if (sameOrChange(ts.commands, index, legacy, expected, c.op)) for (const key of mapped.reads) reads.add(key);
    } else if (c.op === 'fetchIncome')
      sameOrChange(
        ts.commands,
        index,
        { ok: [] },
        { error: 'Coinbase Advanced Trade does not publish an income ledger API for this account path' },
        'income availability',
      );
    else if (['setLeverage', 'setMarginType', 'setPositionMode'].includes(c.op))
      sameOrChange(
        ts.commands,
        index,
        { error: 'Not implemented' },
        { error: 'Coinbase does not support account leverage, margin or position-mode mutations' },
        'account mutation availability',
      );
    else if (c.op === 'fetchLeverage') {
      const market = markets.get(c.symbol);
      assert.ok(market, 'missing Coinbase settings market');
      const leverage = market.product_type === 'SPOT' ? 1 : 10;
      sameOrChange(
        ts.commands,
        index,
        { ok: null },
        { ok: { buyLeverage: leverage, sellLeverage: leverage } },
        'known account leverage',
      );
    } else if (c.op === 'cancelOrders') {
      const candidates = records.filter(
        (r) =>
          r.type === 'http' &&
          r.method === 'POST' &&
          route(r) === '/api/v3/brokerage/orders/batch_cancel' &&
          r.requestBody &&
          isDeepStrictEqual(parse(r.requestBody).order_ids, c.ids),
      );
      assert.equal(candidates.length, 1, 'ambiguous Coinbase cancel oracle');
      const body = parse(candidates[0].body);
      const failures = body.results.filter((r) => !r.success);
      if (failures.length)
        sameOrChange(
          ts.commands,
          index,
          { ok: [] },
          { error: failures.map((r) => `${r.order_id}: ${r.failure_reason ?? 'cancel rejected'}`).join(', ') },
          'partial cancellation rejection',
        );
      else sameOrChange(ts.commands, index, { ok: [] }, { ok: null }, 'successful cancellation void result');
    } else if (c.op === 'amendOrder') {
      const meta = records.find((r) => r.type === 'meta');
      if (meta?.profile === 'cfm')
        sameOrChange(
          ts.commands,
          index,
          { error: `Coinbase cannot safely edit ${c.id}: CFM contract size units are not proven for native edit` },
          { error: 'Coinbase native edit requires reconciled zero-fill spot LIMIT provenance' },
          'unproven CFM native edit',
        );
      if (meta?.profile === 'intx') {
        const confirmation = records.find(
          (r) => r.type === 'http' && r.method === 'GET' && route(r) === `/api/v3/brokerage/orders/historical/${c.id}`,
        );
        assert.ok(confirmation && successful(confirmation), 'missing Coinbase cancellation proof oracle');
        const detail = parse(confirmation.body).order;
        assert.equal(detail.status, 'CANCELLED');
        assert.equal(number(detail.filled_size), 0);
        reads.add(requestKey('GET', confirmation.url));
      }
    }
  }
  if (Array.isArray(ts.requests)) ts.requests = [...new Set([...ts.requests, ...reads])].sort();
}
function correctProductRequests(ts, records) {
  if (!Array.isArray(ts.requests)) return;
  const replacements = new Map();
  for (const record of records) {
    if (!successful(record) || record.method !== 'GET' || route(record) !== '/api/v3/brokerage/products') continue;
    const url = new URL(record.url);
    if (url.searchParams.get('limit') !== '1000') continue;
    const paged = requestKey('GET', record.url);
    url.searchParams.delete('limit');
    replacements.set(requestKey('GET', url), paged);
  }
  for (const record of records) {
    if (
      !successful(record) ||
      record.method !== 'GET' ||
      !/^\/api\/v3\/brokerage\/market\/products\/[^/]+\/candles$/.test(route(record))
    )
      continue;
    const native = requestKey('GET', record.url),
      legacy = new URL(record.url);
    legacy.pathname = legacy.pathname.replace('/brokerage/market/', '/brokerage/');
    legacy.searchParams.delete('limit');
    replacements.set(requestKey('GET', legacy), native);
  }
  ts.requests = [...new Set(ts.requests.map((key) => replacements.get(key) ?? key))].sort();
}
function correctMarketStatus(ts, markets) {
  for (const row of ts.markets ?? []) {
    const raw = markets.get(row.id ?? row.symbol);
    if (!raw) continue;
    assert.equal(typeof raw.status, 'string', 'missing Coinbase product status');
    for (const flag of ['is_disabled', 'trading_disabled'])
      assert.ok(raw[flag] === undefined || typeof raw[flag] === 'boolean', 'invalid Coinbase disabled flag');
    const expected = ['online', 'TRADING'].includes(raw.status) && !raw.is_disabled && !raw.trading_disabled;
    sameOrChange(row, 'active', raw.status === 'TRADING', expected, 'active product classification');
  }
}
export function fixCoinbase(ts, records) {
  assert.ok(Array.isArray(records), 'Coinbase corrections require input records');
  correctProductRequests(ts, records);
  const markets = products(records);
  correctMarketStatus(ts, markets);
  const privateTape =
    records.find((r) => r.type === 'meta')?.private ?? records.some((r) => r.url?.includes('-ws-user.'));
  if (!privateTape) return;
  correctCommands(ts, records, markets);
  if (!ts.account) return;
  correctFills(ts.account, records, markets);
  correctOrders(ts.account, records, markets);
  correctBalances(ts.account, records);
  correctSettings(ts.account, markets);
}
