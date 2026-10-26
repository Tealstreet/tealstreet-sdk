// Documented DX REST payloads, not a live account capture. Quote values are scripted.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { BREAKOUT_ACCOUNT, BREAKOUT_REPLAY_TOKEN } from './breakoutprop-requests.mjs';
export const BREAKOUT_EPOCH = Date.parse('2026-07-15T10:00:00Z');
const ROOT = 'https://breakout.prosp.devexperts.com/dxsca-web';
export const symbols = ['BTCUSD', 'ETHUSD', 'AAVEUSD', 'ACTUSD', 'ADAUSD'];
const zeroCloseOrder = {
  account: BREAKOUT_ACCOUNT, version: 86, orderId: 76515504, orderCode: '4471153:55640',
  clientOrderId: '55640', instrument: 'ETHUSD', type: 'STOP', status: 'WORKING', finalStatus: false,
  transactionTime: '2026-07-15T09:55:49.900Z', stopPrice: 3500,
  legs: [{ instrument: 'ETHUSD', side: 'SELL', positionEffect: 'CLOSE',
    quantity: 0, filledQuantity: 0, remainingQuantity: 0 }],
};
const quotes = (shift = 0) => symbols.map((symbol, i) => ({ symbol, type: 'Quote', bid: [98117.9, 3656.01, 269.09, 0.4806, 1.001][i] + shift, ask: [98118, 3656.02, 269.13, 0.4808, 1.0011][i] + shift, time: new Date(BREAKOUT_EPOCH + shift * 1000).toISOString() }));
export const breakoutPortfolio = (orders = []) => ({ portfolios: [{ account: BREAKOUT_ACCOUNT, version: 86, balances: [{ account: BREAKOUT_ACCOUNT, version: 86, value: 4999.96, currency: 'USD' }], positions: [
  { account: BREAKOUT_ACCOUNT, version: 85, positionCode: '76515503', symbol: 'ETHUSD', quantity: 0.01, side: 'BUY', quantityNotional: 0.01, openPrice: 3656.01, marginRate: 0.2, lastUpdateTime: '2024-12-11T06:00:49.087Z' },
  { account: BREAKOUT_ACCOUNT, version: 82, positionCode: '76515542', symbol: 'ETHUSD', quantity: 0.01, side: 'BUY', quantityNotional: 0.01, openPrice: 3660, marginRate: 0.2, lastUpdateTime: '2024-12-11T05:56:26.400Z' },
], orders }] });
export const buildBreakoutTape = ({ publicOnly = false } = {}) => {
  const records = [
    { t: 0, type: 'meta', epoch: BREAKOUT_EPOCH, private: !publicOnly, source: 'documented-scripted', specification: 'https://demo-xt.dx.trade/specs/DX_REST_API.html' },
    { t: 0, type: 'config', private: !publicOnly, options: { subaccount: '00002' } },
  ];
  const http = (method, path, data, extras = {}) => records.push({ t: 0, type: 'http', method, url: `${ROOT}/${path}`, status: 200, body: JSON.stringify(data), ...extras });
  http('POST', 'login', { sessionToken: BREAKOUT_REPLAY_TOKEN, timeout: '00:30:00' }, { requestBody: JSON.stringify({ domain: 'default', username: 'replay-key', password: 'replay-secret' }) });
  http('GET', 'users/replay-key', { userDetails: [{ login: 'replay-key', domain: 'default', accounts: [{ account: 'default:12', accountStatus: 'READ_ONLY' }, { account: BREAKOUT_ACCOUNT, accountStatus: 'FULL_TRADING' }] }] });
  http('GET', `instruments/query?accountCode=${encodeURIComponent(BREAKOUT_ACCOUNT)}`, { instruments: [...symbols, 'XBTUSD'].map((symbol) => ({ symbol, priceIncrement: symbol === 'ACTUSD' || symbol === 'ADAUSD' ? 0.0001 : 0.01, lotSize: 1 })) });
  http('GET', `accounts/${encodeURIComponent(BREAKOUT_ACCOUNT)}/instruments/query`, { instrumentDetails: [...symbols, 'XBTUSD'].map((symbol) => ({ account: BREAKOUT_ACCOUNT, symbol, tradingStatus: 'FULL', minOrderSize: 0.01, maxOrderSize: { value: 724, limitType: 'LOTS' }, minOrderSizeIncrement: 0.01, marginRate: { rateType: 'FLAT', tiers: [{ endVolume: 'Infinity', value: 0.2 }] } })) });
  const quoteBody = JSON.stringify({ eventTypes: [{ type: 'Quote' }], symbols, account: BREAKOUT_ACCOUNT });
  http('POST', 'marketdata', { events: quotes() }, { requestBody: quoteBody, availableAfter: 0 });
  if (!publicOnly) http('GET', `accounts/${encodeURIComponent(BREAKOUT_ACCOUNT)}/portfolio`, breakoutPortfolio([
    zeroCloseOrder,
    { ...zeroCloseOrder, orderId: 76515505, orderCode: '4471153:55641', clientOrderId: '',
      stopPrice: undefined, legs: [{ ...zeroCloseOrder.legs[0], price: 3500 }] },
  ]), { availableAfter: 0 });
  for (let t = 2000; t <= 20000; t += 2000) http('POST', 'marketdata', { events: quotes(t / 20000) }, { t, availableAfter: t, requestBody: quoteBody });
  if (!publicOnly) {
    records.push({ t: 22000, type: 'command', op: 'placeOrders', orders: [{ symbol: 'ETHUSD', type: 'limit', side: 'buy', amount: 0.0126, price: 3656.019 }] });
    http('POST', `accounts/${encodeURIComponent(BREAKOUT_ACCOUNT)}/orders`, { orderId: 42 }, { t: 22000, afterCommand: 1, requestBody: JSON.stringify({ orderCode: 'tealstreet-documented-generated', type: 'LIMIT', instrument: 'ETHUSD', quantity: 0.01, side: 'BUY', limitPrice: 3656.02, positionEffect: 'OPEN', tif: 'GTC' }) });
    records.push({ t: 23000, type: 'command', op: 'fetchCandles', symbol: 'ETHUSD', interval: '1h', from: BREAKOUT_EPOCH - 3600000, to: BREAKOUT_EPOCH });
    http('POST', 'marketdata', { events: [{ symbol: 'ETHUSD', type: 'Candle', candleType: 'h', open: 3656.01, high: 3657, low: 3655, close: 3656.5, volume: 922.778, time: '2026-07-15T09:00:00Z' }] }, { t: 23000, requestBody: JSON.stringify({ eventTypes: [{ type: 'Candle', candleType: 'h', fromTime: '2026-07-15T09:00:00.000Z', toTime: '2026-07-15T10:00:00.000Z' }], symbols: ['ETHUSD'], account: BREAKOUT_ACCOUNT }) });
    for (const op of ['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchLeverage']) records.push({ t: 24000, type: 'command', op, symbol: 'ETHUSD' });
  }
  return records;
};
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({ options: { out: { type: 'string' }, public: { type: 'boolean', default: false } } });
  const out = values.out ?? `sdk-rs/harness/tapes/breakoutprop-${values.public ? 'public-documented' : 'private'}.jsonl`;
  const records = buildBreakoutTape({ publicOnly: values.public });
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
  console.log(JSON.stringify({ out, records: records.length, source: 'documented-scripted' }));
}
