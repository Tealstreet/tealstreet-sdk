import { createHmac, timingSafeEqual } from 'node:crypto';

const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('hex');
const sameSignature = (actual, expected) => typeof actual === 'string' && /^[a-f0-9]{64}$/i.test(actual) &&
  timingSafeEqual(Buffer.from(actual.toLowerCase()), Buffer.from(expected));
const fresh = (timestamp, window) => Number.isSafeInteger(Number(timestamp)) && /^\d+$/.test(String(timestamp)) &&
  Number(window) > 0 && Number(window) <= 60_000 && Number(timestamp) < Date.now() + 1000 && Date.now() - Number(timestamp) <= Number(window);
const publicPaths = /\/(?:exchangeInfo|ticker\/24hr|ticker\/bookTicker|premiumIndex|depth|klines|trades|historicalTrades|aggTrades|time)$/;

/** Validate the actual signed bytes before canonical request matching drops clocks. */
export const validateBinanceRequest = ({ method, url, body, headers = {} }) => {
  const parsed = new URL(url);
  // Generic identity matching omits several venues' generated IDs. Spot and
  // COIN-M do not accept these aliases, so reject them before canonicalization.
  // USD-M clientAlgoId and PM newClientStrategyId remain native controls.
  if (/^\/(?:api\/v3|dapi\/v[12])\//.test(parsed.pathname) &&
      ['c', 'clientOrderId', 'clientAlgoId', 'requestId'].some((key) => parsed.searchParams.has(key)))
    throw new Error('Unsupported Binance Spot/COIN-M query alias');
  const fields = Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers))
    .map(([key, value]) => [key.toLowerCase(), String(value)]));
  if (body !== undefined && body !== null && body !== '') throw new Error('Binance request parameters must be in the URL');
  if (publicPaths.test(parsed.pathname)) {
    if (fields['x-mbx-apikey'] || parsed.searchParams.has('signature')) throw new Error('Binance public request leaked credentials');
    return;
  }
  if (fields['x-mbx-apikey'] !== 'replay-key') throw new Error('Invalid Binance API key');
  // The engine and TS sign demo listen-key requests; production uses only
  // the API-key header. Demo requests share the full timestamp/HMAC checks.
  const signedListenKey = ['https://demo-fapi.binance.com', 'https://demo-dapi.binance.com'].includes(parsed.origin);
  if (parsed.pathname.endsWith('/listenKey') && ['POST', 'PUT'].includes(method) && !signedListenKey) {
    if (parsed.searchParams.has('signature')) throw new Error('Binance live listen-key request was signed');
    return;
  }
  const params = parsed.searchParams;
  if (['timestamp', 'recvWindow', 'signature'].some((key) => params.getAll(key).length !== 1) ||
      !fresh(params.get('timestamp'), params.get('recvWindow'))) throw new Error('Invalid Binance timestamp/window');
  const signed = parsed.search.slice(1).split('&').filter((part) => part.split('=', 1)[0] !== 'signature').join('&');
  if (!sameSignature(params.get('signature'), hmac(signed))) throw new Error('Invalid Binance request signature');
};

export const spotSocketRequest = (message) => {
  if (!['userDataStream.subscribe.signature', 'session.status', 'session.subscriptions'].includes(message.method)) return;
  return { id: message.id, key: message.method };
};
export const spotSocketResponse = (message) => Number.isInteger(message.status) && 'id' in message ? { id: message.id } : undefined;
const isPrivate = (url) => new URL(url).pathname === '/ws-api/v3';

// Binance Spot WebSocket API: signed subscription, correlated ACK, then a
// subscription-ID-scoped event stream. This oracle never reads engine output.
// https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/user-data-stream-requests
export const createBinanceSpotSocketAudit = (_impl, { isPrivate: required }) => {
  const states = new Map();
  const failures = [];
  const state = (url, generation = 1) => {
    const key = `${url} ${generation}`;
    if (!states.has(key)) states.set(key, { pending: new Map(), subscription: undefined, auth: false, closed: false });
    return states.get(key);
  };
  const fail = (message) => failures.push(message);
  return {
    connected(url, generation) { if (isPrivate(url)) state(url, generation); },
    closed(url, generation) { if (isPrivate(url)) state(url, generation).closed = true; },
    sent(url, text, generation) {
      if (!isPrivate(url)) return;
      const message = JSON.parse(text);
      const s = state(url, generation);
      if (!((typeof message.id === 'string' && message.id.length > 0) || Number.isSafeInteger(message.id)) || s.pending.has(message.id)) {
        fail('Invalid or in-flight reused Binance Spot request ID'); return;
      }
      if (message.method === 'userDataStream.subscribe.signature') {
        const p = message.params;
        const keys = Object.keys(p ?? {}).sort().join(',');
        const payload = `apiKey=${p?.apiKey}&recvWindow=${p?.recvWindow}&timestamp=${p?.timestamp}`;
        s.auth = keys === 'apiKey,recvWindow,signature,timestamp' && p.apiKey === 'replay-key' && p.recvWindow === 5000 &&
          fresh(p.timestamp, p.recvWindow) && sameSignature(p.signature, hmac(payload));
        if (!s.auth) fail('Invalid Binance Spot subscription signature/key/window');
      } else if (message.method === 'session.subscriptions') {
        if (s.subscription === undefined) fail('Binance Spot heartbeat preceded subscription ACK');
        if (message.params !== undefined && Object.keys(message.params).length) fail('Binance Spot heartbeat has unexpected parameters');
      } else fail(`Unexpected Binance Spot private method ${message.method}`);
      s.pending.set(message.id, message.method);
    },
    received(url, text, generation) {
      if (!isPrivate(url)) return;
      const message = JSON.parse(text);
      const s = state(url, generation);
      if ('id' in message) {
        const method = s.pending.get(message.id);
        if (!method) { fail('Uncorrelated Binance Spot acknowledgement'); return; }
        s.pending.delete(message.id);
        if (message.status !== 200) { fail('Binance Spot acknowledgement rejected'); return; }
        if (method === 'userDataStream.subscribe.signature') {
          if (!s.auth || !Number.isSafeInteger(message.result?.subscriptionId) || message.result.subscriptionId < 0)
            fail('Invalid Binance Spot subscription acknowledgement');
          else s.subscription = message.result.subscriptionId;
        } else if (method === 'session.subscriptions') {
          if (!Array.isArray(message.result) || !message.result.some((item) => item.subscriptionId === s.subscription))
            fail('Binance Spot subscription missing from heartbeat');
        }
      } else if (!message.event || message.subscriptionId !== s.subscription || s.subscription === undefined || s.closed)
        fail('Binance Spot event outside acknowledged subscription/generation');
    },
    ready(accountReady = false) { return !required || [...states.values()].some((s) => !s.closed && s.auth && (!accountReady || s.subscription !== undefined)); },
    errors() {
      if (required && ![...states.values()].some((s) => !s.closed && s.subscription !== undefined))
        fail('Binance Spot subscription never acknowledged');
      for (const s of states.values()) if (!s.closed && s.pending.size) fail('Binance Spot request never acknowledged');
      return [...new Set(failures)];
    },
  };
};

/** Replay the documented replacement for retired Spot aggregate feeds through
 * TS's own handlers/store. Transport and handler work stays inside measurement.
 * Original TS still requests !ticker@arr/!bookTicker; production Rust shards
 * per-symbol @ticker subscriptions at Binance's 1024-stream connection limit.
 */
export const prepareBinanceSpotPublic = (exchange, WebSocket) => {
  const feed = exchange.publicWebsocket;
  const sockets = [];
  const retries = new Set();
  let disposed = false;
  const connect = (url, topics, index) => {
    if (disposed || feed.isDisposed) return;
    const socket = new WebSocket(url);
    sockets[index] = socket;
    socket.addEventListener('open', () => {
      const batches = [];
      let batch = [];
      for (const topic of topics) {
        const candidate = [...batch, topic];
        if (batch.length && (candidate.length > 128 || Buffer.byteLength(JSON.stringify({ id: 1, method: 'SUBSCRIBE', params: candidate })) > 4096)) {
          batches.push(batch); batch = [];
        }
        batch.push(topic);
      }
      if (batch.length) batches.push(batch);
      const send = (part) => {
        if (disposed || sockets[index] !== socket) return;
        socket.send(JSON.stringify({ id: part + 1, method: 'SUBSCRIBE', params: batches[part] }));
        if (part + 1 < batches.length) {
          const timer = setTimeout(() => { retries.delete(timer); send(part + 1); }, 400);
          retries.add(timer);
        }
      };
      send(0);
    });
    socket.addEventListener('message', (event) => {
      if (!disposed && sockets[index] === socket) feed.onMessage(event);
    });
    socket.addEventListener('close', () => {
      if (disposed || sockets[index] !== socket) return;
      const timer = setTimeout(() => { retries.delete(timer); connect(url, topics, index); }, 1000);
      retries.add(timer);
    });
  };
  feed.onOpen = () => {
    if (disposed || feed.isDisposed) return;
    feed.resetOrderbook();
    feed.subscribe();
    // The book socket reconnects independently of ticker shards.
    if (sockets.length) return;
    const topics = exchange.store.markets.map((market) => `${market.symbol.toLowerCase()}@ticker`);
    for (let offset = 0; offset < topics.length; offset += 1024)
      connect(feed.ws.url, topics.slice(offset, offset + 1024), offset / 1024);
  };
  const ticker = feed.handleTickerStreamEvents;
  feed.handleTickerStreamEvents = (events) => {
    ticker(events);
    // Per-symbol tickers carry bid/ask; there is no aggregate bookTicker feed.
    feed.handleBookTickersStreamEvents(events);
  };
  const dispose = feed.dispose.bind(feed);
  feed.dispose = () => { disposed = true; for (const timer of retries) clearTimeout(timer); for (const socket of sockets) socket.close(); dispose(); };
};

/** The TS reference uses session.status's authenticated-session flag for a
 * signature-only subscription. Send the documented subscription listing and
 * adapt its verified own-ID result to TS's existing heartbeat handler.
 */
export const prepareBinanceSpotPrivate = (exchange) => {
  const feed = exchange.privateWebsocket;
  let subscription;
  const receive = feed.onMessage;
  const close = feed.onClose;
  feed.ping = () => {
    if (feed.isDisposed) return;
    feed.pingAt = performance.now();
    feed.sendWithLog(JSON.stringify({ id: 42, method: 'session.subscriptions' }));
  };
  feed.onMessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id === 'tealstreet-user-data' && message.status === 200 && Number.isSafeInteger(message.result?.subscriptionId))
      subscription = message.result.subscriptionId;
    if (message.id === 42) {
      const active = message.status === 200 && subscription !== undefined && Array.isArray(message.result) &&
        message.result.some((item) => item.subscriptionId === subscription);
      return receive({ ...event, data: JSON.stringify({ ...message, result: { userDataStream: active } }) });
    }
    receive(event);
  };
  feed.onClose = (...args) => { subscription = undefined; return close(...args); };
};
export const prepareBinanceSpot = (exchange, WebSocket) => {
  prepareBinanceSpotPublic(exchange, WebSocket);
  prepareBinanceSpotPrivate(exchange);
};

// executionReport.p is the resting limit, L is this execution's price.
export const fixBinanceSpotFills = (account, records) => {
  if (!account) return;
  const executions = records.filter((record) => record.type === 'ws' && !record.impl)
    .map((record) => JSON.parse(record.text).event)
    .filter((event) => event?.e === 'executionReport' && event.x === 'TRADE');
  for (let i = 0; i < account.fills.length; i++) {
    const event = executions[i];
    const fill = account.fills[i];
    if (event && fill.id === event.c && fill.symbol === event.s && fill.amount === Number(event.l))
      fill.price = Number(event.L);
  }
};

/** Auth bytes are audited before comparing the nondeterministic wire identity. */
export const binanceSubscriptionMessage = (message) => {
  if (message.method === 'LIST_SUBSCRIPTIONS' && Object.keys(message).every((key) => ['id', 'method'].includes(key)))
    return { method: 'ping' };
  if (message.method === 'userDataStream.subscribe.signature' && message.params)
    return { ...message, params: { ...message.params, timestamp: '<timestamp>', signature: '<signature>' } };
  return message;
};

export const binanceTickerSymbols = (text) => {
  const payload = JSON.parse(text);
  return (Array.isArray(payload) ? payload : [payload]).filter((event) => event?.e === '24hrTicker').map((event) => event.s);
};

// TS cancelSymbolOrders relabels even already-filled rows as canceled. Preserve
// the explicit native terminal event; never infer this correction from Rust.
export const fixBinanceFilledStatus = (account, records, spot) => {
  if (!account) return;
  const filled = new Set();
  const identities = new Map();
  for (const record of records) {
    if (record.type !== 'ws' || record.impl) continue;
    const message = JSON.parse(record.text);
    const event = spot ? message.event : message;
    const order = spot ? event : event?.o;
    if (!(spot ? event?.e === 'executionReport' : event?.e === 'ORDER_TRADE_UPDATE') || order?.i === undefined) continue;
    const key = `${order.s} ${spot ? order.c : order.i}`;
    if (!identities.has(key)) identities.set(key, new Set());
    identities.get(key).add(String(order.i));
    if (order.X === 'FILLED') filled.add(key);
  }
  for (const order of account.orders) {
    const key = `${order.symbol} ${order.id}`;
    // A reused Spot client ID can name a different live native order.
    if (filled.has(key) && identities.get(key)?.size === 1) order.status = 'closed';
  }
};
