// Synthetic Classic USDC-M payloads. This is an authored fixture, never a live
// capture or a renamed USDT tape. Public/private replay use the real USDC venue.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';

export const BITGET_USDC_EPOCH = 1760880000000;
export const BITGET_USDC_SYMBOLS = Object.freeze(['BTCUSDC', 'ETHUSDC']);
const REST = 'https://api.bitget.com';
const PUBLIC = 'wss://ws.bitget.com/v2/ws/public';
const PRIVATE = 'wss://ws.bitget.com/v2/ws/private';
const PRODUCT = 'usdc-futures';
const INSTRUMENT_TYPE = 'USDC-FUTURES';
export const BITGET_USDC_SOURCES = Object.freeze([
  'https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market',
  'https://www.bitget.com/docs/catalog/classic-contract-account/classic-contract-account',
  'https://www.bitget.com/docs/catalog/classic-contract-position/classic-contract-position',
  'https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade',
  'https://www.bitget.com/docs/classic/websocket/contract/public/Tickers-Channel',
  'https://www.bitget.com/docs/classic/websocket/contract/public/Order-Book-Channel',
  'https://www.bitget.com/docs/classic/websocket/contract/private/Order-Channel',
]);
const meta = (privateTape) => ({
  t: 0, type: 'meta', private: privateTape, epoch: BITGET_USDC_EPOCH,
  source: 'synthetic-documented',
  provenance: {
    kind: 'synthetic', liveCapture: false, venue: 'bitgetusdc',
    generator: 'sdk-rs/harness/private-tape-bitgetusdc.mjs',
    productType: PRODUCT, marginCoin: 'USDC', instType: INSTRUMENT_TYPE,
    scope: privateTape ? 'Classic USDC-M account, order lifecycle and bounded queries' : 'Classic USDC-M public snapshots and deltas; bounded correctness fixture, not a load test',
    depthIntegrity: 'contiguous seq/pseq; valid legacy CRC32 included for compatibility with the pre-May-2026 schema',
    sources: BITGET_USDC_SOURCES,
  },
});
const http = (path, data, extras = {}) => ({
  t: 0, type: 'http', method: 'GET', url: REST + path, status: 200,
  body: JSON.stringify({ code: '00000', msg: 'success', requestTime: BITGET_USDC_EPOCH, data }),
  ...extras,
});
const ws = (t, channel, data, extras = {}) => ({
  t, type: 'ws', url: PUBLIC,
  text: JSON.stringify({ arg: { instType: INSTRUMENT_TYPE, channel, instId: data[0].instId }, action: 'snapshot', data, ...extras }),
});
const markets = () => BITGET_USDC_SYMBOLS.map((symbol, index) => ({
  symbol, baseCoin: index ? 'ETH' : 'BTC', quoteCoin: 'USDC', supportMarginCoins: ['USDC'],
  symbolStatus: 'normal', symbolType: 'perpetual', pricePlace: index ? '2' : '1',
  priceEndStep: '1', volumePlace: index ? '2' : '3', sizeMultiplier: index ? '0.01' : '0.001',
  minTradeNum: index ? '0.01' : '0.001', maxOrderQty: index ? '900' : '120',
  maxMarketOrderQty: index ? '500' : '100', minLever: '1', maxLever: '125', fundInterval: '8',
}));
const tickers = (stage = 0) => BITGET_USDC_SYMBOLS.map((symbol, index) => ({
  symbol, instId: symbol, lastPr: index ? '2500' : '50000',
  bidPr: index ? '2499.99' : '49999.9', askPr: index ? '2500.01' : '50000.1',
  markPrice: index ? '2500' : '50000', indexPrice: index ? '2499' : '49990',
  changeUtc24h: index ? '-0.02' : '0.01', fundingRate: index ? '0.0002' : '0.0001',
  baseVolume: String(1000 + stage), quoteVolume: String((1000 + stage) * (index ? 2500 : 50000)),
  nextFundingTime: String(BITGET_USDC_EPOCH + 8 * 3600000), ts: String(BITGET_USDC_EPOCH + stage * 250),
}));
const publicReads = () => [
  http(`/api/v2/mix/market/contracts?productType=${PRODUCT}`, markets()),
  http(`/api/v2/mix/market/tickers?productType=${PRODUCT}`, tickers()),
];

// Bitget's legacy checksum interleaves bid then ask price/size for the first
// 25 levels. Values remain wire strings so decimal formatting is preserved.
export function bitgetUsdcChecksum(bids, asks) {
  const cells = [];
  for (let index = 0; index < 25; index++)
    for (const level of [bids[index], asks[index]]) if (level) cells.push(...level);
  let crc = 0xffffffff;
  for (const byte of Buffer.from(cells.join(':'), 'utf8')) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) | 0;
}

export function buildBitgetUsdcPublicTape() {
  const records = [meta(false), ...publicReads()];
  const states = BITGET_USDC_SYMBOLS.map((_, index) => ({
    bids: index ? [['2499.99', '2'], ['2499.98', '1']] : [['49999.9', '2'], ['49999.8', '1']],
    asks: index ? [['2500.01', '1.5'], ['2500.02', '3']] : [['50000.1', '1.5'], ['50000.2', '3']],
  }));
  // Nine declared stages exercise both symbols, absolute depth replacement,
  // level removal/reinsertion, and rolling ticker quantities without bulk data.
  for (let stage = 0; stage < 9; stage++) {
    for (const [index, row] of tickers(stage).entries()) {
      records.push(ws(100 + stage * 250, 'ticker', [row]));
      const state = states[index];
      let bids = structuredClone(state.bids), asks = structuredClone(state.asks);
      if (stage) {
        state.bids[0][1] = String(2 + stage / 10);
        state.asks[0][1] = String(1.5 + stage / 10);
        const second = index ? '2499.98' : '49999.8';
        bids = [[state.bids[0][0], state.bids[0][1]], [second, stage % 2 ? '0' : '1']];
        asks = [[state.asks[0][0], state.asks[0][1]]];
        state.bids = stage % 2 ? [state.bids[0]] : [state.bids[0], [second, '1']];
      }
      records.push(ws(110 + stage * 250, 'books', [{
        instId: row.instId, bids, asks, seq: String(1000 + stage), pseq: String(stage ? 999 + stage : 0),
        checksum: bitgetUsdcChecksum(state.bids, state.asks), ts: String(BITGET_USDC_EPOCH + stage * 250),
      }], { action: stage ? 'update' : 'snapshot' }));
    }
  }
  return records;
}

export function buildBitgetUsdcPrivateTape() {
  const records = [meta(true), ...publicReads()];
  const get = (path, data, extras) => records.push(http(path, data, extras));
  const post = (path, requestBody, data, extras = {}) => get(path, data, { method: 'POST', requestBody, ...extras });
  const message = (t, channel, data, action = 'snapshot') => records.push({
    t, type: 'ws', url: PRIVATE,
    text: JSON.stringify({ arg: { instType: INSTRUMENT_TYPE, channel, instId: 'default' }, action, data }),
  });
  const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
  const positions = BITGET_USDC_SYMBOLS.map((symbol, index) => ({
    symbol, instId: symbol, marginCoin: 'USDC', holdSide: index ? 'short' : 'long',
    total: index ? '1' : '0.1', openPriceAvg: index ? '2600' : '49000', markPrice: index ? '2500' : '50000',
    leverage: '20', unrealizedPL: '100', liquidationPrice: index ? '3900' : '30000',
    marginSize: index ? '130' : '245', marginMode: 'crossed', posMode: 'one_way_mode',
    achievedProfits: index ? '2' : '3', totalFee: '-0.1', deductedFee: '0.01', uTime: String(BITGET_USDC_EPOCH),
  }));
  const order = (id, symbol, price, size, clientOid) => ({
    orderId: id, clientOid, symbol, instId: symbol, size, baseVolume: '0', price,
    side: 'buy', orderType: 'limit', status: 'live', force: 'gtc', reduceOnly: 'NO',
    posMode: 'one_way_mode', cTime: String(BITGET_USDC_EPOCH), uTime: String(BITGET_USDC_EPOCH),
  });
  const resting = [order('usdc-1001', 'BTCUSDC', '49000', '0.002', 'resting-btc'), order('usdc-2001', 'ETHUSDC', '2400', '0.02', 'resting-eth')];
  get('/api/v3/account/settings', null, { body: JSON.stringify({ code: '40084', msg: 'Classic Account mode', data: null }) });
  get(`/api/v2/mix/position/all-position?productType=${PRODUCT}&marginCoin=USDC`, positions);
  get(`/api/v2/mix/account/accounts?productType=${PRODUCT}`, [{ marginCoin: 'USDC', unionTotalMargin: '10200', unionAvailable: '9000', unrealizedPL: '200', accountEquity: '10200', available: '9000' }]);
  get(`/api/v2/mix/order/orders-pending?productType=${PRODUCT}&limit=200`, { entrustedList: resting, endId: '' });
  for (const planType of ['profit_loss', 'track_plan'])
    get(`/api/v2/mix/order/orders-plan-pending?productType=${PRODUCT}&planType=${planType}&limit=200`, { entrustedList: [], endId: '' });
  for (const symbol of BITGET_USDC_SYMBOLS)
    get(`/api/v2/mix/account/account?symbol=${symbol}&productType=${PRODUCT}&marginCoin=USDC`, { symbol, marginCoin: 'USDC', posMode: 'one_way_mode', marginMode: 'crossed', leverage: '20' });
  records.push({ t: 0, type: 'ws', url: PRIVATE, text: JSON.stringify({ event: 'login', code: '0', connId: 'synthetic-usdc' }) });
  for (const row of tickers()) records.push(ws(100, 'ticker', [row]));
  message(200, 'positions', positions);
  message(300, 'orders', resting);
  command(600, 'placeOrders', { orders: [{ symbol: 'BTCUSDC', type: 'limit', side: 'buy', amount: 0.003, price: 49100, clientOrderId: 'caller:usdc-1' }] });
  const body = { symbol: 'BTCUSDC', productType: PRODUCT, marginCoin: 'USDC', marginMode: 'crossed', orderList: [{ size: '0.003', price: '49100', side: 'buy', orderType: 'limit', force: 'gtc', reduceOnly: 'NO', clientOid: 'caller:usdc-1' }] };
  post('/api/v2/mix/order/batch-place-order', body, { successList: [{ orderId: 'usdc-1002', clientOid: 'caller:usdc-1' }], failureList: [] });
  // Declare each implementation's existing spelling; the tape matcher audits
  // the entire selected payload, including USDC collateral and native product.
  records.at(-1).tsRequestBody = { ...body, marginMode: 'cross' };
  const created = order('usdc-1002', 'BTCUSDC', '49100', '0.003', 'caller:usdc-1');
  message(1000, 'orders', [created]);
  command(1400, 'amendOrder', { id: 'usdc-1002', price: 49200 });
  post('/api/v2/mix/order/modify-order', { orderId: 'usdc-1002', symbol: 'BTCUSDC', productType: PRODUCT, newPrice: '49200', newSize: '0.003', newClientOid: 'generated' }, { orderId: 'usdc-1003' });
  message(1800, 'orders', [{ ...created, status: 'canceled' }]);
  const amended = order('usdc-1003', 'BTCUSDC', '49200', '0.003', 'generated');
  message(2000, 'orders', [amended]);
  command(2400, 'cancelOrders', { ids: ['usdc-1003'] });
  post('/api/v2/mix/order/batch-cancel-orders', { symbol: 'BTCUSDC', orderIdList: [{ orderId: 'usdc-1003' }], marginCoin: 'USDC', productType: PRODUCT }, { successList: [{ orderId: 'usdc-1003' }], failureList: [] });
  message(2800, 'orders', [{ ...amended, status: 'canceled' }]);
  message(3200, 'orders', [{ ...resting[0], tradeId: 'usdc-execution-1', status: 'filled', baseVolume: '0.002', accBaseVolume: '0.002', fillPrice: '49000', priceAvg: '49000', fillFee: '-0.01', fillFeeCoin: 'USDC', fillTime: String(BITGET_USDC_EPOCH + 3200) }]);
  command(6000, 'fetchFills', { symbol: 'BTCUSDC', from: BITGET_USDC_EPOCH, to: BITGET_USDC_EPOCH + 60000, limit: 10 });
  get(`/api/v2/mix/order/fills?productType=${PRODUCT}&limit=10&symbol=BTCUSDC&startTime=${BITGET_USDC_EPOCH}&endTime=${BITGET_USDC_EPOCH + 60000}`, {
    fillList: [{ tradeId: 'usdc-execution-1', orderId: 'usdc-1001', symbol: 'BTCUSDC', price: '49000', baseVolume: '0.002', quoteVolume: '98', side: 'buy', tradeSide: 'buy_single', posMode: 'one_way_mode', profit: '0', feeDetail: [{ feeCoin: 'USDC', totalFee: '-0.01' }], tradeScope: 'maker', cTime: String(BITGET_USDC_EPOCH + 3200) }], endId: '',
  });
  command(7000, 'getAccount');
  get('/api/v2/spot/account/info', { userId: 'usdc-user', parentId: '', inviterId: 'synthetic-referrer' });
  const finalOrders = [order('usdc-1004', 'BTCUSDC', '49300', '0.004', 'external-usdc'), resting[1]];
  get(`/api/v2/mix/order/orders-pending?productType=${PRODUCT}&limit=200`, { entrustedList: finalOrders, endId: '' }, { availableAfter: 10000 });
  message(10000, 'orders', finalOrders);
  return records;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const { values } = parseArgs({ options: { out: { type: 'string' }, public: { type: 'boolean', default: false } } });
  const out = values.out ?? `sdk-rs/harness/tapes/bitgetusdc-${values.public ? 'public' : 'private'}.jsonl`;
  const records = values.public ? buildBitgetUsdcPublicTape() : buildBitgetUsdcPrivateTape();
  const text = records.map((record) => JSON.stringify(record)).join('\n') + '\n';
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, out.endsWith('.gz') ? gzipSync(text, { level: 9, mtime: 0 }) : text);
  console.log(JSON.stringify({ out, records: records.length, frames: records.filter((record) => record.type === 'ws').length, source: 'synthetic-documented', liveCapture: false }));
}
