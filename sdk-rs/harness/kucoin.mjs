import { createHmac } from 'node:crypto';
const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('base64');
const privateTopics = ['/contractMarket/tradeOrders', '/contractMarket/advancedOrders', '/contract/positionAll', '/contractAccount/wallet', '/contract/marginMode', '/contract/crossLeverage'];
const pendingReads = new WeakMap();
export const kucoin = {
  rust: 'kucoin',
  ts: { module: '../../packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts', className: 'KucoinExchange', name: 'kucoin' },
  configureTs(exchange) {
    // Track before the 3 RPS queue: accelerated frames can finish while cached
    // depth recovery reads are still waiting to be issued.
    const pending = new Set();
    pendingReads.set(exchange, pending);
    const get = exchange.xhr.get.bind(exchange.xhr);
    exchange.xhr.get = (...args) => {
      const promise = get(...args);
      pending.add(promise);
      promise.then(() => pending.delete(promise), () => pending.delete(promise));
      return promise;
    };
  },
  async settleTs(exchange, { isPrivate, clock }) {
    if (isPrivate) return;
    const pending = pendingReads.get(exchange);
    let stable = 0;
    while (stable < 3) {
      if (clock?.active) {
        await clock.advance(clock.time - clock.epoch + 1);
        await new Promise(setImmediate);
      }
      else await new Promise((resolve) => setTimeout(resolve, 1));
      stable = pending.size ? 0 : stable + 1;
    }
  },
  startupMessage: (record) => { try { return JSON.parse(record.text).type === 'welcome'; } catch { return false; } },
  volatileQueryFields: ['connectId'],
  matchRequestBody: true,
  generatedBodyId: (key, value) => key === 'clientOid' && typeof value === 'string',
  isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/bullet-'),
  isPrivateRecord: (record) => {
    if (record.type !== 'ws') return false;
    try { return privateTopics.includes(JSON.parse(record.text).topic) || new URL(record.url).searchParams.get('token')?.startsWith('private-'); }
    catch { return false; }
  },
  compareFillPrices: true,
  respondToJsonPing: true,
  tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderBook_')).length >= count,
  publicTickers: (records) => {
    const seed = records.find((r) => r.type === 'http' && r.url.endsWith('/api/v1/contracts/active'));
    return seed ? JSON.parse(seed.body).data.filter((m) => (m.settleCurrency || m.quoteCurrency || m.quoteCoin) === 'USDT' && !m.expireDate).map((m) => m.symbol) : [];
  },
  checkRequest(method, url, body, headers) {
    const header = Object.fromEntries(Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]));
    const parsed = new URL(url);
    const publicRead = ['/api/v1/contracts/active', '/api/v1/allTickers', '/api/v1/level2/snapshot', '/api/v1/kline/query', '/api/v1/trade/history', '/api/v1/bullet-public'].includes(parsed.pathname);
    if (!header['kc-api-key']) return publicRead;
    const timestamp = header['kc-api-timestamp'];
    const path = parsed.pathname + (parsed.search ? '?' + [...parsed.searchParams].map(([k, v]) => `${k}=${v}`).join('&') : '');
    return header['kc-api-key'] === 'replay-key' && header['kc-api-key-version'] === '2' &&
      header['kc-api-passphrase'] === hmac('replay-passphrase') && /^\d+$/.test(timestamp ?? '') &&
      Math.abs(Date.now() - Number(timestamp)) < 60_000 && header['kc-api-sign'] === hmac(timestamp + method + path + (body ?? ''));
  },
  createSocketAudit(impl, { isPrivate, books, tickers = [], records = [] }) {
    const tokens = new Map();
    for (const r of records.filter((r) => r.type === 'http' && r.url.includes('/bullet-'))) {
      const data = JSON.parse(r.body).data;
      if (data?.token) tokens.set(data.token, { private: r.url.endsWith('/bullet-private'), endpoint: data.instanceServers?.[0]?.endpoint });
    }
    const connections = new Map();
    const failures = [];
    const state = (url) => {
      if (!connections.has(url)) {
        const parsed = new URL(url), token = tokens.get(parsed.searchParams.get('token'));
        if (!token || !url.startsWith(token.endpoint + '?')) failures.push('socket did not use bullet token and endpoint');
        if (!parsed.searchParams.get('connectId')) failures.push('socket missing connectId');
        connections.set(url, { private: token?.private ?? false, topics: new Set(), pings: new Set(), welcomed: false });
      }
      return connections.get(url);
    };
    const has = (topic) => [...connections.values()].some((c) => c.topics.has(topic));
    return {
      sent(url, text) {
        const s = state(url), m = JSON.parse(text);
        if (impl === 'rust' && ['subscribe', 'ping'].includes(m.type) && !s.welcomed) failures.push('KuCoin sent before welcome');
        if (m.type === 'login' || m.type === 'auth') failures.push('unexpected post-token authentication');
        if (m.type === 'ping') { if (m.id === undefined) failures.push('ping missing id'); s.pings.add(String(m.id)); }
        if (['subscribe', 'unsubscribe'].includes(m.type)) {
          if (Boolean(m.privateChannel) !== s.private) failures.push('subscription privateChannel does not match bullet token');
          const [channel, symbols] = m.topic.split(':');
          const topics = symbols ? symbols.split(',').map((symbol) => `${channel}:${symbol}`) : [channel];
          if (topics.length > 100) failures.push('subscription exceeds 100 symbols per message');
          for (const topic of topics) if (m.type === 'subscribe') s.topics.add(topic); else s.topics.delete(topic);
          if (s.topics.size > 400) failures.push('socket exceeds 400 subscriptions');
        }
      },
      received(url, text) {
        const s = state(url), m = JSON.parse(text);
        if (m.type === 'welcome') s.welcomed = true;
        if (m.type === 'pong' && m.id !== undefined) s.pings.delete(String(m.id));
        if (m.type === 'message' && m.topic && !s.topics.has(m.topic)) failures.push(`unsubscribed KuCoin topic ${m.topic}`);
      },
      ready() {
        return books.every((symbol) => {
          const api = symbol === 'BTCUSDT' ? 'XBTUSDTM' : symbol + 'M';
          return ['/contractMarket/level2:', '/contractMarket/tickerV2:', '/contract/instrument:'].every((channel) => has(channel + api));
        }) &&
          tickers.every((symbol) => has('/contractMarket/snapshot:' + symbol)) &&
          (!isPrivate || privateTopics.every(has));
      },
      errors() {
        if (!this.ready()) failures.push('missing KuCoin subscriptions');
        return [...new Set(failures)];
      },
    };
  },
};
