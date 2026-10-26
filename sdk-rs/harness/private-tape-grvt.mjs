// GRVT documented lite REST and private feeds: https://api-docs.grvt.io/trading_api/
// Public deterministic signing credentials; this replay never contacts an account.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/grvt-private.jsonl' } } });
export const EPOCH = 1700000000000;
export const CLIENT = '9223372036854775807';
export const BUILDER = '0xee8f3559f298fffa79b8253437969d15b5dccff3';
const REST = 'https://trades.grvt.io';
const PUBLIC = 'wss://market-data.grvt.io/ws/full';
const PRIVATE = 'wss://trades.grvt.io/ws';
const records = [{ t: 0, type: 'meta', epoch: EPOCH, private: true }];
let time = 0;
const nanos = (ms) => String(BigInt(EPOCH + ms) * 1000000n);
const http = (path, data, requestBody, options = {}) => records.push({
  t: 0, type: 'http', method: 'POST', url: path.startsWith('https://') ? path : REST + path,
  status: 200, body: JSON.stringify(data), requestBody, ...options,
});
const ws = (url, data) => records.push({ t: (time += 100), type: 'ws', url, text: typeof data === 'string' ? data : JSON.stringify(data) });
const command = (data) => records.push({ t: (time += 100), type: 'command', ...data });
const instrument = (base, hash) => ({ i: `${base}_USDT_Perp`, ih: hash, b: base, q: 'USDT', k: 'PERPETUAL',
  ts: '0.1', ms: '0.001', mp: '1000', bd: 9, qd: 6, ia: true });
const instruments = [instrument('BTC', '0x030501'), instrument('ETH', '0x030502')];
http('https://market-data.grvt.io/lite/v1/all_instruments', { r: instruments }, {});
const login = (index, afterCommand = 0) => http('https://edge.grvt.io/auth/api_key/login', { status: 'success' }, { api_key: 'grvt-replay-key' }, {
  headers: `Set-Cookie: gravity=fixture-session-${index}; Path=/; Secure; HttpOnly\nX-Ts-Grvt-Session: fixture-session-${index}\nX-Grvt-Account-Id: fixture-account`, afterCommand,
});
login(1);
http('/lite/v1/aggregated_account_summary', { r: { te: '1000', ts2: '800', tu: '20', sas: [{ si: '777' }] } }, {});
const leverage = (btc) => ({ r: [{ i: 'BTC_USDT_Perp', l: String(btc), ml: '1', ml1: '50', mt: 'ISOLATED' },
  { i: 'ETH_USDT_Perp', l: '10', ml: '1', ml1: '50', mt: 'ISOLATED' }] });
http('/lite/v1/get_all_initial_leverage', leverage(20), { sa: '777' });
http('/full/v1/get_authorized_builders', { results: [{ builder_account_id: BUILDER, max_futures_fee_rate: '0.03' }] }, {});
const position = (lev) => ({ i: 'BTC_USDT_Perp', sa: '777', s: '2', ep: '100', mp: '110', n: '220', up: '20', rp: '0', l: String(lev), el: '50', mt: 'ISOLATED' });
http('/lite/v1/positions', { r: [position(20)] }, { sa: '777' });
const order = (client, native, status = 'OPEN', updated = 0, patch = {}) => ({ oi: native, sa: '777', im: false, ro: false,
  po: false, ti: 'GOOD_TILL_TIME', l: [{ i: 'BTC_USDT_Perp', s: '0.004', lp: '90', ib: true }],
  m: { co: client, ct: nanos(0) }, s1: { s: status, bs: [status === 'OPEN' ? '0.004' : '0'], ts: ['0'], ut: nanos(updated) }, ...patch });
const seed = order('100', '0x100');
// A supported ETH perpetual remains live when only BTC orders are cancelled.
const sibling = order('101', '0x101', 'OPEN', 0, { l: [{ i: 'ETH_USDT_Perp', s: '0.004', lp: '90', ib: true }] });
http('/lite/v1/open_orders', { r: [seed, sibling] }, { sa: '777' });
// Only a valid first-frame proxy authentication can unlock the three feeds.
ws(PRIVATE, { type: 'auth_success' });
for (const { i } of instruments) {
  const price = i === 'ETH_USDT_Perp' ? '200' : '110';
  ws(PUBLIC, {
  s: 'v1.ticker.d', s1: i, sn: '0',
  f: { i, lp: price, mp: price, ip: price, bb: price, ba: price, op: price, bv: '2', sv: '3' },
});
}
ws(PRIVATE, 'ping');
const fill = (trade, amount, price) => ws(PRIVATE, { s: 'v1.fill', s1: '777-BTC_USDT_Perp', f: {
  sa: '777', i: 'BTC_USDT_Perp', ti: trade, oi: '0x700', co: '700', ib: true, s: amount, p: price,
} });
fill('9223372036854775806', '0.25', '110');
fill('9223372036854775806', '0.25', '110'); // exact execution duplicate
fill('9223372036854775805', '0.1', '111'); // an individual execution, not cumulative
command({ op: 'placeOrders', orders: [{ symbol: 'BTC_USDT_PERP', type: 'limit', side: 'buy', amount: 0.004,
  price: 90, timeInForce: 'GoodTillCancel', clientOrderId: CLIENT }] });
// Signature randomness and clocks are independently verified before matching.
const signed = { o: { sa: '777', im: false, ti: 'GOOD_TILL_TIME', po: false, ro: false,
  l: [{ i: 'BTC_USDT_Perp', s: '0.004', lp: '90', ib: true }],
  s: { s: '0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1', r: '<r>', s1: '<s1>', v: 27, e: '<e>', n: '<n>', ci: '325' },
  m: { co: CLIENT, ct: '<ct>' }, b: BUILDER, bf: '0.03' } };
// HTTP 401 with explicit GRVT code 1000 proves rejection and permits one
// session refresh/resend of the identical signed order and caller identity.
http('/lite/v1/create_order', { c: 1000, m: 'Session expired', s: 'UNAUTHORIZED' }, signed, { status: 401, afterCommand: 1 });
login(2, 1);
http('/lite/v1/create_order', { r: { oi: '0x300', m: { co: CLIENT } } }, signed, { afterCommand: 1 });
http('/lite/v1/open_orders', { r: [seed, sibling, order(CLIENT, '0x300', 'OPEN', 2000)] }, { sa: '777' }, { afterCommand: 1 });
ws(PRIVATE, { type: 'auth_success' });
ws(PRIVATE, { s: 'v1.order', s1: '777-BTC_USDT_Perp', sn: '0', f: order(CLIENT, '0x300', 'OPEN', 2500) });
command({ op: 'cancelOrders', ids: [CLIENT] });
http('/lite/v1/cancel_order', { r: { a: true } }, { sa: '777', oi: '0x300' }, { afterCommand: 2 });
http('/lite/v1/open_orders', { r: [seed, sibling] }, { sa: '777' }, { afterCommand: 2 });
ws(PRIVATE, { s: 'v1.order', s1: '777-BTC_USDT_Perp', sn: '0', f: order(CLIENT, '0x300', 'CANCELLED', 3000) });
command({ op: 'setLeverage', symbol: 'BTC_USDT_PERP', leverage: 15 });
http('/lite/v1/set_initial_leverage', { s: true }, { sa: '777', i: 'BTC_USDT_Perp', l: '15' }, { afterCommand: 3 });
http('/lite/v1/get_all_initial_leverage', leverage(15), { sa: '777' }, { afterCommand: 3 });
http('/lite/v1/positions', { r: [position(15)] }, { sa: '777' }, { afterCommand: 3 });
ws(PRIVATE, 'ping');
ws(PRIVATE, 'pong');
command({ op: 'cancelSymbolOrders', symbol: 'BTC_USDT_PERP' });
// Exact-symbol cancellation reads the entire authoritative subaccount, then
// cancels native IDs individually. The bulk API has no instrument selector.
http('/lite/v1/open_orders', { r: [seed, sibling] }, { sa: '777' }, { afterCommand: 4 });
http('/lite/v1/cancel_order', { r: { a: true } }, { sa: '777', oi: '0x100' }, { afterCommand: 4 });
http('/lite/v1/open_orders', { r: [sibling] }, { sa: '777' }, { afterCommand: 4 });
ws(PRIVATE, { s: 'v1.order', s1: '777-BTC_USDT_Perp', sn: '0', f: order('100', '0x100', 'CANCELLED', 4000) });
// ETH remains untouched by the BTC command; its later external cancellation
// arrives independently before the empty-store global cancellation control.
ws(PRIVATE, { s: 'v1.order', s1: '777-ETH_USDT_Perp', sn: '0', f: order('101', '0x101', 'CANCELLED', 4500,
  { l: [{ i: 'ETH_USDT_Perp', s: '0.004', lp: '90', ib: true }] }) });
// Native cancel-all must still execute after the local order store is empty.
command({ op: 'cancelSymbolOrders', symbol: '' });
http('/lite/v1/cancel_all_orders', { r: { a: true } }, { sa: '777' }, { afterCommand: 5 });
http('/lite/v1/open_orders', { r: [] }, { sa: '777' }, { afterCommand: 5 });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map(JSON.stringify).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
