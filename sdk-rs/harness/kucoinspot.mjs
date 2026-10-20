// Current KuCoin Spot HF adapter contract; no output normalizations.
import { createHmac } from 'node:crypto';
import { requestKey } from './requests.mjs';
import { uniqueRequestHeaders } from './protocol-headers.mjs';
const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('base64');
const privateTopics = ['/spotMarket/tradeOrdersV2', '/account/balance'];
export const kucoinspot = {
  rust: 'kucoinspot',
  ts: { module: '../../packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts', className: 'KucoinSpotExchange', name: 'kucoinspot' },
  startupMessage: (record) => { try { return JSON.parse(record.text).type === 'welcome'; } catch { return false; } },
  volatileQueryFields: ['connectId'],
  matchRequestBody: true,
  requestKey(method, url, body, callerIds) { return requestKey(method, url, body, callerIds, this); },
  requestOptions: (method, url) => ({
    // connectId is socket transport state; no REST query field is generated.
    volatileQueryFields: [],
    preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
    preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
    generatedId: () => false,
    generatedBodyId: (key, value) => method === 'POST' && url.pathname === '/api/v1/hf/orders'
      && key === 'clientOid' && typeof value === 'string' && /^[A-Za-z0-9]{1,21}$/.test(value),
  }),
  isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/bullet-'),
  isPrivateRecord: (record) => {
    if (record.type !== 'ws') return false;
    try { return privateTopics.includes(JSON.parse(record.text).topic) || new URL(record.url).searchParams.get('token')?.startsWith('private-'); }
    catch { return false; }
  },
  compareFillPrices: true,
  compareBalanceCoins: true,
  compareBalanceCurrencies: true,
  compareWriteOrder: true,
  respondToJsonPing: true,
  tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderBook_')).length >= count,
  checkRequest(method, url, body, headers) {
    let header;
    try { header = uniqueRequestHeaders(headers); } catch { return false; }
    const parsed = new URL(url);
    if (parsed.origin !== 'https://api.kucoin.com' || parsed.username || parsed.password || parsed.href.includes('#')) return false;
    const publicRead = ['/api/v2/symbols', '/api/v1/market/allTickers', '/api/v1/market/candles', '/api/v1/bullet-public'].includes(parsed.pathname);
    if (!header['kc-api-key']) return publicRead;
    const timestamp = header['kc-api-timestamp'];
    const path = parsed.pathname + (parsed.search ? '?' + [...parsed.searchParams].map(([k, v]) => `${k}=${v}`).join('&') : '');
    return header['kc-api-key'] === 'replay-key' && header['kc-api-key-version'] === '2' &&
      header['kc-api-passphrase'] === hmac('replay-passphrase') && /^\d+$/.test(timestamp ?? '') &&
      Math.abs(Date.now() - Number(timestamp)) < 60_000 && header['kc-api-sign'] === hmac(timestamp + method + path + (body ?? ''));
  },
  createSocketAudit(impl, { isPrivate, books, records = [] }) {
    const tokens = new Map();
    for (const r of records.filter((r) => r.type === 'http' && r.url.includes('/bullet-'))) {
      const data = JSON.parse(r.body).data;
      if (data?.token) tokens.set(data.token, { private: r.url.endsWith('/bullet-private'), endpoint: data.instanceServers?.[0]?.endpoint });
    }
    const connections = new Map(); const failures = [];
    const state = (url, socket) => {
      const identity = socket ?? url;
      if (!connections.has(identity)) {
        const parsed = new URL(url), token = tokens.get(parsed.searchParams.get('token'));
        if (!token || !url.startsWith(token.endpoint + '?')) failures.push('spot socket must use bullet token and server');
        if (!parsed.searchParams.get('connectId')) failures.push('spot socket missing connectId');
        connections.set(identity, { socket, closed: false, private: token?.private ?? false, topics: new Set(), welcomed: false });
      }
      return connections.get(identity);
    };
    const has = (topic) => [...connections.values()].some((c) => !c.closed && !c.socket?.closed && c.topics.has(topic));
    return {
      connected(url, socket) { state(url, socket); },
      closed(url, socket) { state(url, socket).closed = true; },
      sent(url, text, socket) {
        const s = state(url, socket), m = JSON.parse(text);
        if (s.closed || s.socket?.closed) failures.push('spot sent on closed socket');
        if (impl === 'rust' && ['subscribe', 'ping'].includes(m.type) && !s.welcomed) failures.push('spot sent before welcome');
        if (['auth', 'login'].includes(m.type)) failures.push('unexpected spot socket login');
        if (m.type === 'ping' && m.id === undefined) failures.push('spot ping missing id');
        if (['subscribe', 'unsubscribe'].includes(m.type)) {
          if (Boolean(m.privateChannel) !== s.private) failures.push('spot topic channel disagrees with bullet role');
          if (m.type === 'subscribe') s.topics.add(m.topic); else s.topics.delete(m.topic);
          if (s.topics.size > 400) failures.push('too many spot subscriptions');
        }
      },
      received(url, text, socket) {
        const s = state(url, socket), m = JSON.parse(text);
        if (s.closed || s.socket?.closed) failures.push('spot received on closed socket');
        if (m.type === 'welcome') s.welcomed = true;
        if (m.type === 'message' && m.topic && !s.topics.has(m.topic)) failures.push(`unsubscribed spot topic ${m.topic}`);
      },
      ready() { return has('/market/ticker:all') && books.every((symbol) => has('/spotMarket/level2Depth50:' + symbol)) && (!isPrivate || privateTopics.every(has)); },
      errors() { if (!this.ready()) failures.push('missing spot subscriptions'); return [...new Set(failures)]; },
    };
  },
};
