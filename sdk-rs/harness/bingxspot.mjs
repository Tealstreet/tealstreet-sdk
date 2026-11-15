import { createHmac } from 'node:crypto';
import { gunzipSync } from 'node:zlib';
import { requestKey } from './requests.mjs';

const publicPaths = new Set(['/openApi/spot/v1/common/symbols', '/openApi/spot/v2/quote/ticker',
  '/openApi/spot/v2/quote/klines', '/openApi/spot/v1/market/depth']);
export const bingxspot = {
  rust: 'bingxspot',
  rustFixture: 'bingxSpotReplayFixture',
  virtualPrivateClock: true,
  ts: { module: './bingxspot-reference.mjs', className: 'BingxSpotReplayExchange', name: 'bingxspot' },
  privateSocket: '?listenKey=',
  bingxProtocol: true,
  compressedTextSockets: true,
  subscriptionGenerations: true,
  matchRequestBody: true,
  compareWriteOrder: true,
  compareBalanceCurrencies: true,
  compareFillPrices: true,
  requestKey(method, url, body, callerIds) { return requestKey(method, url, body, callerIds, this); },
  isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/userDataStream'),
  tsBooksReady: exchange => Boolean(exchange.public),
  tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).length >= count,
  settleTs: exchange => exchange.settle(),
  createSocketAudit(_impl, { isPrivate, books }) {
    const sockets = new Map(), failures = [];
    const state = (url, generation = 1) => {
      const key = `${url} ${generation}`;
      if (!sockets.has(key)) {
        const parsed = new URL(url);
        if (parsed.origin !== 'wss://open-api-ws.bingx.com' || parsed.pathname !== '/market') failures.push('non-Spot socket');
        if (parsed.search && parsed.searchParams.get('listenKey') !== 'spot-fixture-session') failures.push('wrong Spot listen-key session');
        sockets.set(key, { private: parsed.searchParams.has('listenKey'), channels: new Set() });
      }
      return sockets.get(key);
    };
    const required = ['BTC-USDT@ticker', ...books.map(s => `${s.replace(/USDT$/, '-USDT')}@depth50`)];
    const has = (channel, privateSocket) => [...sockets.values()].some(s => s.private === privateSocket && s.channels.has(channel));
    return {
      sent(url, text, generation) {
        if (['Ping', 'Pong'].includes(text)) return;
        const socket = state(url, generation), message = JSON.parse(text);
        if (!['sub', 'unsub'].includes(message.reqType) || typeof message.dataType !== 'string') {
          failures.push('invalid Spot subscription'); return;
        }
        if (socket.private !== ['spot.executionReport', 'ACCOUNT_UPDATE'].includes(message.dataType)) failures.push('Spot subscription on wrong socket role');
        if (message.reqType === 'sub') socket.channels.add(message.dataType);
        else socket.channels.delete(message.dataType);
      },
      received(url, text, generation) {
        const socket = state(url, generation);
        if (typeof text !== 'string') text = gunzipSync(Buffer.from(text)).toString('utf8');
        if (['Ping', 'Pong'].includes(text)) return;
        const message = JSON.parse(text);
        if (message.dataType && !socket.channels.has(message.dataType)) failures.push('unsubscribed Spot data');
      },
      ready() { return required.every(channel => has(channel, false)) && (!isPrivate || ['spot.executionReport', 'ACCOUNT_UPDATE'].every(channel => has(channel, true))); },
      errors() { return [...failures, ...(!this.ready() ? ['missing Spot subscriptions'] : [])]; },
    };
  },
  verifyRequest({ method, url, headers, body }) {
    const parsed = new URL(url);
    if (parsed.origin !== 'https://open-api.bingx.com') return false;
    if (publicPaths.has(parsed.pathname)) return method === 'GET';
    if (!parsed.pathname.startsWith('/openApi/spot/') && parsed.pathname !== '/openApi/user/auth/userDataStream') return false;
    const key = Object.entries(headers ?? {}).find(([key]) => key.toLowerCase() === 'x-bx-apikey')?.[1];
    if (key !== 'replay-key') return false;
    const fields = method === 'GET' ? Object.fromEntries(parsed.searchParams) : JSON.parse(body || '{}');
    if (parsed.pathname.endsWith('/userDataStream')) return fields.signature === undefined && fields.timestamp === undefined;
    const { signature, ...params } = fields;
    if (!/^\d+$/.test(String(params.timestamp)) || Math.abs(Date.now() - Number(params.timestamp)) >= 60_000) return false;
    const canonical = Object.entries(params).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
      .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`).join('&');
    return signature === createHmac('sha256', 'replay-secret').update(canonical).digest('hex');
  },
};
