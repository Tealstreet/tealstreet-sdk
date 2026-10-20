// Official /info, /exchange and websocket shapes, deterministic public test key.
// https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { HYPERLIQUID_SPOT_REPLAY } from './hyperliquidspot.mjs';
export const SPOT_EPOCH = 1792454400000;
export const hyperliquidSpotMeta = [{
  tokens: [{ name: 'USDC', szDecimals: 8, index: 0, weiDecimals: 8 }, { name: 'PURR', szDecimals: 2, index: 1, weiDecimals: 5 }, { name: 'HYPE', szDecimals: 2, index: 150, weiDecimals: 8 }],
  universe: [{ name: '@1', index: 1, tokens: [1, 0], isCanonical: true }, { name: '@107', index: 107, tokens: [150, 0], isCanonical: true }],
}, ['@1', '@107'].map((coin) => ({ coin, midPx: '100', markPx: '100', prevDayPx: '100', dayNtlVlm: '10000', dayBaseVlm: '100', circulatingSupply: '1000000' }))];
export const hyperliquidSpotBalance = { balances: [{ coin: 'USDC', token: 0, hold: '100', total: '1000' }, { coin: 'PURR', token: 1, hold: '0.5', total: '2' }] };
export function buildHyperliquidSpotTape() {
  const records = [
    { t: 0, type: 'meta', epoch: SPOT_EPOCH, private: true, source: 'documented-scripted', specification: 'https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/spot' },
    { t: 0, type: 'config', private: true, options: HYPERLIQUID_SPOT_REPLAY },
  ];
  const http = (path, request, body, extra = {}) => records.push({ t: 0, type: 'http', method: 'POST', url: `https://api.hyperliquid.xyz${path}`, requestBody: JSON.stringify(request), status: 200, body: JSON.stringify(body), ...extra });
  const info = (type, body, extra = {}) => http('/info', { type, ...extra }, body);
  const ws = (t, channel, data) => records.push({ t, type: 'ws', url: 'wss://api.hyperliquid.xyz/ws', text: JSON.stringify({ channel, data }) });
  const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
  info('spotMetaAndAssetCtxs', hyperliquidSpotMeta);
  info('spotClearinghouseState', hyperliquidSpotBalance, { user: HYPERLIQUID_SPOT_REPLAY.applicationId });
  // Rust reconciles standalone spot open orders; none rest in this scenario.
  info('frontendOpenOrders', [], { user: HYPERLIQUID_SPOT_REPLAY.applicationId });
  ws(100, 'allMids', { mids: { '@1': '100', '@107': '100' } });
  ws(200, 'spotState', { user: HYPERLIQUID_SPOT_REPLAY.applicationId, spotState: hyperliquidSpotBalance });
  const fill = { coin: '@1', px: '100', sz: '0.5', side: 'B', time: SPOT_EPOCH + 300, startPosition: '0', dir: 'Buy', closedPnl: '0', oid: 900, tid: 12345, crossed: true, fee: '0.01', feeToken: 'USDC', hash: `0x${'11'.repeat(32)}` };
  ws(300, 'userFills', { user: HYPERLIQUID_SPOT_REPLAY.applicationId, isSnapshot: true, fills: [fill] });
  ws(400, 'userFills', { user: HYPERLIQUID_SPOT_REPLAY.applicationId, isSnapshot: false, fills: [fill] });
  const caller = `0x${'01'.repeat(16)}`;
  command(1000, 'placeOrders', { orders: [{ symbol: 'PURR/USDC', type: 'market', side: 'buy', amount: 0.25, clientOrderId: caller }] });
  http('/exchange', { action: { type: 'order', orders: [{ a: 10001, b: true, p: '105', s: '0.25', r: false, t: { limit: { tif: 'Gtc' } }, c: caller }], grouping: 'na' }, nonce: SPOT_EPOCH, signature: { r: '0x00', s: '0x00', v: 27 } },
    { status: 'ok', response: { type: 'order', data: { statuses: [{ filled: { oid: 1001, totalSz: '0.25', avgPx: '100' } }] } } });
  for (const op of ['fetchFills', 'fetchOrderHistory', 'fetchIncome']) command(2000, op, { symbol: 'PURR/USDC' });
  command(2400, 'fetchCandles', { symbol: 'PURR/USDC', interval: '1m', from: SPOT_EPOCH - 60000, to: SPOT_EPOCH });
  http('/info', { type: 'candleSnapshot', req: { coin: '@1', interval: '1m', startTime: SPOT_EPOCH - 60000, endTime: SPOT_EPOCH } },
    [{ t: SPOT_EPOCH - 60000, T: SPOT_EPOCH - 1, s: '@1', i: '1m', o: '99', h: '101', l: '98', c: '100', v: '12', n: 3 }]);
  ws(3000, 'spotState', { user: HYPERLIQUID_SPOT_REPLAY.applicationId, spotState: hyperliquidSpotBalance });
  return records;
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/hyperliquidspot-private.jsonl' } } });
  const records = buildHyperliquidSpotTape();
  mkdirSync(dirname(values.out), { recursive: true });
  writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
  console.log(JSON.stringify({ out: values.out, records: records.length, source: 'documented-scripted' }));
}
