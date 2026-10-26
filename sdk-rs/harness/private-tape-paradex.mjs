// Documented Paradex REST/JSON-RPC payloads and official Python SBE test frames.
// https://docs.paradex.trade/api/prod and tradeparadex/paradex-py; synthetic key=1.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/paradex-private.jsonl' } } });
const REST = 'https://api.prod.paradex.trade/v1';
const SOCKET = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2';
const BTC = 'BTC-USD-PERP', ETH = 'ETH-USD-PERP';
const epoch = 1710000000000;
const records = [{ t: 0, type: 'meta', epoch, private: true }];
let time = 0, commands = 0;
// A write belongs to the command just issued; its stage holds back that
// command's snapshots until the venue acknowledges it.
const http = (method, path, body, requestBody, afterCommand = method !== 'GET' && !path.startsWith('/auth/') && commands ? commands : undefined, extra = {}) =>
  records.push({ t: 0, type: 'http', method, url: REST + path,
  status: 200, body: JSON.stringify(body), ...(requestBody === undefined ? {} : { requestBody }),
  ...(afterCommand === undefined ? {} : { afterCommand }), ...extra });
const ws = (channel, data) => records.push({ t: (time += 100), type: 'ws', url: SOCKET,
  text: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel, data } }) });
const command = (body) => { commands++; records.push({ t: (time += 200), type: 'command', ...body }); };
const order = (id, patch = {}) => ({ id, market: BTC, side: 'BUY', type: 'LIMIT', status: 'OPEN', size: '0.01', remaining_size: '0.01',
  price: '50000', instruction: 'GTC', time_in_force: 'GTC', avg_fill_price: '', flags: [], client_id: '',
  created_at: epoch, last_updated_at: epoch, cancel_reason: '', ...patch });
const initial = order('1000', { side: 'SELL', price: '60000', client_id: 'existing-paradex' });
const snapshot = (rows, stage = commands) => http('GET', '/orders', { results: rows }, undefined, stage);
const payload = (patch = {}) => ({ market: BTC, side: 'BUY', type: 'LIMIT', size: '0.01', price: '50000',
  instruction: 'GTC', stp: 'EXPIRE_TAKER', client_id: 'generated', ...patch });
http('GET', '/system/config', { starknet_chain_id: 'PRIVATE_SN_PARACLEAR_MAINNET', l1_chain_id: '1',
  starknet_fullnode_rpc_url: 'https://rpc.api.prod.paradex.trade/rpc/v0_10', paraclear_address: '0x1', paraclear_decimals: 8,
  paraclear_account_hash: '0x123', paraclear_account_proxy_hash: '0x456', bridged_tokens: [] });
http('POST', '/auth/0x456', { jwt_token: 'replay-paradex-jwt' });
http('GET', '/markets', { results: [BTC, ETH].map((symbol) => ({ symbol, asset_kind: 'PERP',
  base_currency: symbol.split('-')[0], quote_currency: 'USD', order_size_increment: '0.001', price_tick_size: '0.1',
  min_notional: '10', max_order_size: '100', delta1_cross_margin_params: { imf_base: '0.02' } })) });
http('GET', '/markets/summary?market=ALL', { results: [BTC, ETH].map((symbol) => ({ symbol, bid: symbol === BTC ? '49999' : '2999',
  ask: symbol === BTC ? '50001' : '3001', last_traded_price: symbol === BTC ? '50000' : '3000',
  mark_price: symbol === BTC ? '50000' : '3000', underlying_price: symbol === BTC ? '50000' : '3000',
  price_change_rate_24h: '0.01', volume_24h: '10000', open_interest: '20', funding_rate: '0.0001' })) });
http('GET', '/account/margin', { configs: [BTC, ETH].map((market) => ({ market, leverage: 10, margin_type: 'CROSS' })) });
http('GET', '/positions', { results: [] });
http('GET', '/account', { total_collateral: '1000', free_collateral: '900', initial_margin_requirement: '100', account_value: '1000' });
snapshot([initial], 0);
ws(`bbo.${BTC}`, { market: BTC, bid_price: '49999', ask_price: '50001' });
ws(`trades.${BTC}`, { market: BTC, price: '50000', size: '0.01', created_at: epoch });
command({ op: 'placeOrders', orders: [{ symbol: BTC, side: 'buy', type: 'limit', amount: 0.01, price: 50000, clientOrderId: 'caller-paradex-1' }] });
http('POST', '/orders/batch', { orders: [{ id: '1001', client_id: '$request:0.client_id' }], errors: [] }, [payload({ client_id: 'caller-paradex-1' })]);
const placed = order('1001', { client_id: 'caller-paradex-1' });
snapshot([initial, placed]); ws('orders.ALL', placed);
command({ op: 'amendOrder', id: '1001', price: 50010, amount: 0.02 });
http('PUT', '/orders/1001', { id: '1001', client_id: 'caller-paradex-1' }, {
  id: '1001', market: BTC, side: 'BUY', type: 'LIMIT', size: '0.02', price: '50010',
});
const amended = order('1001', { client_id: 'caller-paradex-1', size: '0.02', remaining_size: '0.02', price: '50010' });
snapshot([initial, amended]); ws('orders.ALL', amended);
command({ op: 'cancelOrders', ids: ['1001'] });
http('DELETE', '/orders/1001', { id: '1001', status: 'QUEUED_FOR_CANCELLATION' });
snapshot([initial]); ws('orders.ALL', { ...amended, status: 'CLOSED', cancel_reason: 'USER_CANCELED' });
command({ op: 'placeOrders', orders: [
  { symbol: BTC, side: 'buy', type: 'limit', amount: 0.01, price: 50000, clientOrderId: 'caller-batch-btc' },
  { symbol: ETH, side: 'sell', type: 'limit', amount: 0.02, price: 3000, clientOrderId: 'caller-batch-eth' },
] });
http('POST', '/orders/batch', { orders: [{ id: '2002', client_id: '$request:1.client_id' }, { id: '2001', client_id: '$request:0.client_id' }], errors: [] }, [
  payload({ client_id: 'caller-batch-btc' }), payload({ market: ETH, side: 'SELL', size: '0.02', price: '3000', client_id: 'caller-batch-eth' }),
]);
const batch = [order('2001', { client_id: 'caller-batch-btc' }), order('2002', { market: ETH, side: 'SELL', price: '3000', size: '0.02', remaining_size: '0.02', client_id: 'caller-batch-eth' })];
snapshot([initial, ...batch]); for (const row of batch) ws('orders.ALL', row);
command({ op: 'cancelOrders', ids: ['2001', '2002'] });
http('DELETE', '/orders/batch', { results: [{ id: '2002', status: 'ALREADY_CLOSED' }, { id: '2001', status: 'QUEUED_FOR_CANCELLATION' }] }, { order_ids: ['2001', '2002'] });
snapshot([initial]); for (const row of batch) ws('orders.ALL', { ...row, status: 'CLOSED', cancel_reason: 'USER_CANCELED' });
command({ op: 'placeOrders', orders: [{ symbol: BTC, side: 'buy', type: 'market', amount: 0.005, clientOrderId: 'caller-market' }] });
const marketPayload = payload({ type: 'MARKET', size: '0.005', client_id: 'caller-market' }); delete marketPayload.price;
http('POST', '/orders', { id: '3001', client_id: '$client_id' }, marketPayload);
snapshot([initial]);
const fill = { id: 'fill-3001-a', order_id: '3001', market: BTC, side: 'BUY', price: '50001', size: '0.002',
  remaining_size: '0.003', created_at: epoch + time, fee: '0.01', fee_currency: 'USDC', liquidity: 'TAKER', realized_pnl: '2', realized_funding: '-0.005' };
ws('fills.ALL', fill); ws('fills.ALL', fill); ws('fills.ALL', { ...fill, id: 'fill-3001-b', size: '0.003', remaining_size: '0' });
command({ op: 'placeOrders', orders: [{ symbol: BTC, side: 'buy', type: 'conditional', amount: 0.01, price: 55010, triggerPrice: 55000 }] });
http('POST', '/orders', { id: '4001', client_id: '$client_id' }, payload({ type: 'STOP_LIMIT', price: '55010', trigger_price: '55000' }));
snapshot([initial]);
command({ op: 'cancelSymbolOrders', symbol: BTC });
http('DELETE', `/orders?market=${BTC}`, {}); snapshot([]);
ws('orders.ALL', { ...initial, status: 'CLOSED', cancel_reason: 'USER_CANCELED' });
command({ op: 'setLeverage', symbol: ETH, leverage: 20 });
http('POST', `/account/margin/${ETH}`, {}, { leverage: 20, margin_type: 'CROSS' });
http('GET', '/account/margin', { configs: [{ market: BTC, leverage: 10, margin_type: 'CROSS' }, { market: ETH, leverage: 20, margin_type: 'CROSS' }] }, undefined, commands);
command({ op: 'setMarginType', symbol: BTC, isolated: false });
command({ op: 'setPositionMode', hedged: false });
const from = epoch - 1000, to = epoch + 10000;
command({ op: 'fetchFills', symbol: BTC, from, to, limit: 10 });
const fills = { results: [fill, fill, { ...fill, id: 'fill-3001-b', size: '0.003' }], next: '' };
// Rust bounds fills by the start time too, so a sparse window cannot walk
// older pages it then discards; TS sends only the end.
http('GET', `/fills?market=${BTC}&page_size=10&end_at=${to}`, fills, undefined, undefined, { impl: 'ts' });
http('GET', `/fills?market=${BTC}&page_size=10&start_at=${from}&end_at=${to}`, fills, undefined, undefined, { impl: 'rust' });
const history = [order('3001', { type: 'MARKET', status: 'CLOSED', price: '', size: '0.005', remaining_size: '0', avg_fill_price: '50001', created_at: epoch + 1000 }),
  order('1001', { status: 'CLOSED', remaining_size: '0.005', cancel_reason: 'USER_CANCELED', avg_fill_price: '50000', created_at: epoch + 500 })];
command({ op: 'fetchOrderHistory', symbol: BTC, from, to });
http('GET', `/orders-history?market=${BTC}&start_at=${from}&end_at=${to}&page_size=100`, { results: history, next: '' });
command({ op: 'fetchIncome', symbol: BTC, from, to });
http('GET', `/fills?market=${BTC}&start_at=${from}&end_at=${to}&page_size=500`, { results: [fill, { ...fill, id: 'rebate-2', fee: '-0.002', realized_pnl: '0', realized_funding: '0' }], next: '' });
command({ op: 'fetchPublicTrades', symbol: BTC, limit: 2 });
http('GET', `/trades?market=${BTC}&page_size=2`, { results: [{ id: 'trade-1', market: BTC, side: 'BUY', price: '50001', size: '0.002', created_at: epoch + 1000 }] });
// Exercise real binary private decoding using independently verified official fixtures.
const sbe = JSON.parse(readFileSync('packages/safe-cex/src/__tests__/fixtures/paradex-sbe.json', 'utf8'));
for (const name of ['order-v2', 'fill-v2', 'position-short', 'account-v2']) {
  const example = sbe.cases.find((row) => row.name === name);
  const data = Buffer.from(example.base64, 'base64');
  const accountOffset = { 'order-v2': 76, 'fill-v2': 59, 'position-short': 81, 'account-v2': 72 }[name];
  data.fill(0, 8 + accountOffset, 8 + accountOffset + 32); data[8 + accountOffset + 30] = 1; data[8 + accountOffset + 31] = 0x23;
  records.push({ t: (time += 100), type: 'ws', url: SOCKET, binary: data.toString('base64') });
  if (name === 'position-short') {
    const p = example.expected;
    http('GET', '/positions', { results: [{ market: p.market, side: p.side, status: p.status, size: p.size,
      average_entry_price: p.avg_entry_price, unrealized_pnl: p.unrealized_pnl, realized_positional_pnl: p.realized_pnl,
      liquidation_price: p.liquidation_price ?? '0', leverage: p.leverage, last_updated_at: p.updated_at }] }, undefined, commands);
    records.at(-1).availableAfter = time;
  }
}
// Observe the final state after the venue's one-second order reconciliation.
records.push({ type: 'wait', t: time + 1500 });
records.sort((a, b) => a.t - b.t);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, records.map(JSON.stringify).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length, commands }));
