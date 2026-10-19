// Documented Exchange v1 payloads; no credentials or live writes.
// https://exchange-developer.crypto.com/exchange/v1/
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/cryptocom-private.jsonl' } } });
const REST = 'https://api.crypto.com/exchange/v1/';
const PUBLIC = 'wss://stream.crypto.com/exchange/v1/market';
const PRIVATE = 'wss://stream.crypto.com/exchange/v1/user';
const records = [];
const ok = (result) => ({ code: 0, result });
const get = (path, data) => records.push({ t: 0, type: 'http', method: 'GET', url: REST + path, status: 200, body: JSON.stringify(ok({ data })) });
const post = (path, params, result) => records.push({ t: 0, type: 'http', method: 'POST', url: REST + path, requestBody: JSON.stringify({ method: path, params }), status: 200, body: JSON.stringify(ok(result)) });
let clock = 1200;
const ws = (url, body) => records.push({ t: clock += 300, type: 'ws', url, text: JSON.stringify(body) });
const message = (channel, data) => ({ code: 0, result: { channel, data } });
const command = (body) => records.push({ t: clock += 300, type: 'command', ...body });
const instrument = (symbol, base, priceTick) => ({
  symbol, base_ccy: base, quote_ccy: 'USD', inst_type: 'PERPETUAL_SWAP', tradable: true,
  price_tick_size: priceTick, qty_tick_size: '0.001', quote_decimals: 2, quantity_decimals: 3,
  max_leverage: '50', underlying_symbol: base + 'USD-INDEX', contract_size: '1',
});
get('public/get-instruments', [instrument('BTCUSD-PERP', 'BTC', '0.1'), instrument('ETHUSD-PERP', 'ETH', '0.01')]);
get('public/get-tickers', [
  { i: 'BTCUSD-PERP', b: '95999.9', k: '96000', a: '96000', c: '0.012', v: '1000', vv: '96000000', oi: '100' },
  { i: 'ETHUSD-PERP', b: '2999.99', k: '3000', a: '3000', c: '-0.005', v: '50000', vv: '150000000', oi: '200' },
]);
post('private/get-positions', {}, { data: [{ instrument_name: 'BTCUSD-PERP', type: 'PERPETUAL_SWAP', quantity: '0.01', cost: '950', open_position_pnl: '10', isolation_type: 'CROSS_MARGIN', update_timestamp_ms: 1610000000000 }] });
const balance = { account_id: 'account-123', total_margin_balance: '5010', total_available_balance: '4952', total_initial_margin: '48', total_session_unrealized_pnl: '10', position_balances: [] };
post('private/user-balance', {}, { data: [balance] });
const order = (order_id, instrument_name, side, quantity, limit_price, extra = {}) => ({
  order_id, instrument_name, side, quantity, limit_price, order_type: 'LIMIT', time_in_force: 'GOOD_TILL_CANCEL',
  cumulative_quantity: '0', status: 'ACTIVE', create_time: 1610000000000, update_time: 1610000000000, ...extra,
});
post('private/get-open-orders', {}, { data: [order('9007199254740993', 'BTCUSD-PERP', 'BUY', '0.004', '90000', { client_oid: 'resting', cumulative_quantity: '0.001' })] });
post('private/advanced/get-open-orders', {}, { data: [] });

ws(PRIVATE, { id: 1, method: 'public/auth', code: 0 });
ws(PUBLIC, { code: 0, result: { channel: 'ticker', instrument_name: 'ETHUSD-PERP', data: [{ i: 'ETHUSD-PERP', b: '3000', k: '3001', a: '3000.5', c: '0.001', v: '50001', vv: '150000001', oi: '201' }] } });
ws(PRIVATE, message('user.positions', [{ instrument_name: 'BTCUSD-PERP', quantity: '0.01', cost: '950', leverage: '20', open_position_pnl: '12.5', liquidation_price: '80000', update_timestamp_ms: 1610000001000 }]));
const eth = order('1001', 'ETHUSD-PERP', 'BUY', '1', '3000', { client_oid: 'web' });
ws(PRIVATE, message('user.order', [eth]));
const trade = (trade_id, qty, price) => ({ trade_id, order_id: '1001', instrument_name: 'ETHUSD-PERP', side: 'BUY', traded_quantity: qty, traded_price: price, fees: '-0.01', fee_instrument_name: 'USD', create_time: 1610000001000 });
ws(PRIVATE, message('user.trade', [trade('9007199254740997', '0.4', '2999.5')]));
ws(PRIVATE, message('user.order', [{ ...eth, cumulative_quantity: '0.4', update_time: 1610000002000 }]));
ws(PRIVATE, message('user.trade', [trade('9007199254740998', '0.6', '2999')]));
ws(PRIVATE, message('user.order', [{ ...eth, cumulative_quantity: '1', status: 'FILLED', update_time: 1610000003000 }]));
const stop = order('2001', 'ETHUSD-PERP', 'SELL', '1', '0', { order_type: 'STOP_LOSS', ref_price: '2900', exec_inst: ['REDUCE_ONLY'] });
ws(PRIVATE, message('user.advance.order', [stop]));
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSD-PERP', type: 'limit', side: 'buy', amount: 0.0021, price: 90500.04, clientOrderId: 'caller-123' }] });
post('private/create-order', { instrument_name: 'BTCUSD-PERP', side: 'BUY', type: 'LIMIT', quantity: '0.0021', price: '90500.04', client_oid: 'caller-123' }, { order_id: '1003' });
const placed = order('1003', 'BTCUSD-PERP', 'BUY', '0.0021', '90500.04', { client_oid: 'caller-123', update_time: 1610000004000 });
ws(PRIVATE, message('user.order', [placed]));
command({ op: 'amendOrder', id: '1003', price: 90510 });
post('private/amend-order', { order_id: '1003', instrument_name: 'BTCUSD-PERP', new_quantity: '0.0021', new_price: '90510' }, { order_id: '1003' });
ws(PRIVATE, message('user.order', [{ ...placed, limit_price: '90510', update_time: 1610000005000 }]));
command({ op: 'cancelOrders', ids: ['1003'] });
post('private/cancel-order', { instrument_name: 'BTCUSD-PERP', order_id: '1003' }, {});
ws(PRIVATE, message('user.order', [{ ...placed, limit_price: '90510', status: 'CANCELED', update_time: 1610000006000 }]));
command({ op: 'cancelOrders', ids: ['2001'] });
post('private/advanced/cancel-order', { order_id: '2001' }, {});
ws(PRIVATE, message('user.advance.order', [{ ...stop, status: 'CANCELED', update_time: 1610000006000 }]));
command({ op: 'cancelSymbolOrders', symbol: 'ETHUSD-PERP' });
post('private/cancel-all-orders', { instrument_name: 'ETHUSD-PERP' }, {});
ws(PRIVATE, message('user.positions', [{ instrument_name: 'ETHUSD-PERP', quantity: '-1', cost: '2999.2', leverage: '10', isolation_type: 'ISOLATED_MARGIN', liquidation_price: '4000', open_position_pnl: '1.8', update_timestamp_ms: 1610000007000 }]));
ws(PRIVATE, message('user.balance', [{ ...balance, total_margin_balance: '5020', total_available_balance: '4900', total_initial_margin: '120', total_session_unrealized_pnl: '20' }]));
ws(PRIVATE, { id: 1587523073344, method: 'public/heartbeat', code: 0 });

mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
