// Independent Spot protocol oracle: official REST auth and WS v2 contracts.
import { spotRequestKey } from './spot-request-key.mjs';
import { uniqueRequestHeaders } from './protocol-headers.mjs';
import { createHash, createHmac } from 'node:crypto';
export const KRAKEN_SPOT_REPLAY = { key: 'replay-key', secret: 'cmVwbGF5LXNlY3JldA==' };
export const krakenSpotSignature = (path, body, secret = KRAKEN_SPOT_REPLAY.secret) =>
  createHmac('sha512', Buffer.from(secret, 'base64'))
    .update(path).update(createHash('sha256').update(new URLSearchParams(body).get('nonce') + body).digest())
    .digest('base64');

export function createKrakenSpotRequestAudit(state = {}) {
  let previous = 0n;
  const minted = new Set();
  return ({ method, url, body, headers = {}, now = Date.now() }) => {
    const u = new URL(url);
    if (u.username || u.password || u.hash || new Set(u.searchParams.keys()).size !== [...u.searchParams.keys()].length) throw Error('Kraken Spot unexpected URL fields');
    if (u.origin !== 'https://api.kraken.com') throw Error('Kraken Spot request must use upstream host');
    const h = uniqueRequestHeaders(headers);
    if (u.pathname.startsWith('/0/public/')) {
      if (method !== 'GET' || Object.keys(h).some((name) => ['api-key','api-sign','authorization','coinbase-authorization','cookie'].includes(name))) throw Error('Kraken Spot public credential leak or method');
      return;
    }
    if (!u.pathname.startsWith('/0/private/') || method !== 'POST' || typeof body !== 'string')
      throw Error('Kraken Spot private request must use encoded POST');
    if (u.search) throw Error('Kraken Spot private query parameters unsupported');
    const form = new URLSearchParams(body), nonce = form.get('nonce');
    if (new Set(form.keys()).size !== [...form.keys()].length) throw Error('Kraken Spot duplicate parameter');
    if (h['api-key'] !== KRAKEN_SPOT_REPLAY.key || (typeof nonce !== 'string' || !nonce.length || /[^0-9]/.test(nonce))) throw Error('Kraken Spot missing authentication');
    const n = BigInt(nonce);
    if (n <= previous || Math.abs(Number(n / 1000n) - now) > 60_000) throw Error('Kraken Spot nonce must be fresh and increasing');
    if (h['api-sign'] !== krakenSpotSignature(u.pathname, body)) throw Error('Kraken Spot signature must cover exact form and path');
    previous = n;
    if (u.pathname.endsWith('/GetWebSocketsToken')) state.tokenRequests = (state.tokenRequests ?? 0) + 1;
    if (u.pathname.endsWith('/AddOrder')) {
      const id = form.get('cl_ord_id');
      if ((typeof id !== 'string' || id.length !== 32 || /[^0-9a-f]/.test(id)) || minted.has(id) || form.has('userref'))
        throw Error('Kraken Spot generated client ID missing, reused or conflicting');
      minted.add(id);
    }
    if (u.pathname.endsWith('/AmendOrder') && (!form.get('txid') || form.has('cl_ord_id')))
      throw Error('Kraken Spot native amend must preserve txid');
  };
}

export function validateKrakenSpotSocketMessage(socket, text, _now, state) {
  const frame = JSON.parse(text);
  if (!['subscribe', 'unsubscribe'].includes(frame.method)) return;
  const p = frame.params ?? {};
  if (!['executions', 'balances'].includes(p.channel)) return;
  const expected = state?.tokenRequests > 1 ? 'replay-spot-token-2' : 'replay-spot-token';
  if (new URL(socket.url).origin !== 'wss://ws-auth.kraken.com' || p.token !== expected)
    throw Error('Kraken Spot private subscription must use current REST token');
  if (state && !socket.spotTokenRequest) {
    if ((state.tokenRequests ?? 0) <= (state.usedTokenRequests ?? 0)) throw Error('Kraken Spot reconnect must fetch a fresh token');
    socket.spotTokenRequest = state.tokenRequests;
    state.usedTokenRequests = state.tokenRequests;
  }
  socket.spotChannels ??= new Set();
  if (frame.method === 'subscribe') socket.spotChannels.add(p.channel);
  else socket.spotChannels.delete(p.channel);
}
export function validateKrakenSpotSocketFrame(socket, text) {
  const frame = JSON.parse(text);
  if (['executions', 'balances'].includes(frame.channel) && !socket.spotChannels?.has(frame.channel))
    throw Error('Kraken Spot data arrived before authenticated subscription');
}
export const krakenSpotRequestBody = (body, { method = 'POST', url = 'https://api.kraken.com/0/private/AddOrder', callerIds = new Set() } = {}) => {
  const result = { ...body };
  const route = new URL(url).pathname;
  if (method === 'POST' && route.startsWith('/0/private/')) delete result.nonce;
  const id = result.cl_ord_id;
  if (method === 'POST' && route === '/0/private/AddOrder' && typeof id === 'string' && id.length === 32 && !/[^0-9a-f]/.test(id) && !callerIds.has(id)) result.cl_ord_id = '<generated>';
  return result;
};
export function krakenSpotCommandResult(result, command) {
  if (command.op === 'placeOrders' && command.orders?.some((order) => order.clientOrderId !== undefined)
    && /caller.*client|client.*IDs/i.test(result.error ?? ''))
    return { error: 'caller client order ID unsupported' };
  // TS replay represents a void cancellation as []; the native SDK uses null.
  // Economic ACK validation and one-use taped writes run before this label.
  if (['cancelOrders', 'cancelSymbolOrders'].includes(command.op) && result.ok === null && result.error === undefined)
    return { ok: [] };
  return result;
}
export const krakenspot = {
  rust: 'krakenspot',
  recordAfterReady: true,
  ts: { module: '../../packages/safe-cex/src/exchanges/kraken/krakenspot.exchange.ts', className: 'KrakenSpotExchange', name: 'krakenspot' },
  replayCredentials: KRAKEN_SPOT_REPLAY,
  privateSocket: 'ws-auth.kraken.com',
  requestBodies: true,
  compareWriteOrder: true,
  compareFillPrices: true,
  compareBalanceCurrencies: true,
  // Frames retain numeric currencies; TS-only raw response metadata is opaque.
  tsBalanceCurrencies: (currencies) => Object.fromEntries(Object.entries(currencies).map(([asset, row]) =>
    [asset, { free: row.free, used: row.used, total: row.total, upnl: row.upnl }])),
  createRequestAudit: createKrakenSpotTransportAudit,
  createSocketAudit: createKrakenSpotSocketAudit,
  requestKey: (method, url, body, callerIds) => spotRequestKey(method, url, body, callerIds, krakenSpotRequestBody),
  requestBody: krakenSpotRequestBody,
  commandResult: krakenSpotCommandResult,
  isWrite: (method, url) => method !== 'GET' && !/\/(Balance|OpenOrders|GetWebSocketsToken)$/.test(new URL(url).pathname),
  // start() creates its final public socket only after the ticker seed. Earlier
  // book listeners would attach to the lazy socket that startup replaces.
  tsBooksReady: (exchange, store) => Boolean(store.loaded.tickers && exchange.publicWebsocket),
  tsBooksSubscribed: (exchange, count) => Object.values(exchange.publicWebsocket?.orderBookHandlers ?? {}).filter((v) => v.length).length >= count,
};

export function createKrakenSpotTransportAudit() {
  const state = { tokenRequests: 0 };
  return {
    state,
    validate: createKrakenSpotRequestAudit(),
    response({ url, response }) {
      if (new URL(url).pathname !== '/0/private/GetWebSocketsToken' || response.status !== 200) return;
      const payload = JSON.parse(response.body);
      if (!Array.isArray(payload.error) || payload.error.length || typeof payload.result?.token !== 'string' || !payload.result.token) throw Error('Kraken Spot invalid token acknowledgement');
      state.tokenRequests++;
      state.token = payload.result.token;
    },
  };
}
export function createKrakenSpotSocketAudit(_impl, { isPrivate, requestAudit } = {}) {
  const states = new Map(), failures = [], tokenState = requestAudit?.state ?? {};
  const state = (socket) => {
    if (!states.has(socket)) states.set(socket, { closed: false, topics: new Set() });
    return states.get(socket);
  };
  const checkUrl = (url) => {
    const u = new URL(url);
    if (!['wss://ws.kraken.com','wss://ws-auth.kraken.com'].includes(u.origin) || u.pathname !== '/v2' || u.search || u.hash || u.username || u.password) throw Error('Kraken Spot invalid socket URL');
    return u.hostname === 'ws-auth.kraken.com';
  };
  return {
    connected(url, socket) { checkUrl(url); state(socket); },
    closed(_url, socket) { state(socket).closed = true; },
    sent(url, text, socket) {
      const privateSocket = checkUrl(url), own = state(socket), frame = JSON.parse(text);
      if (own.closed) throw Error('Kraken Spot send on closed socket');
      if (frame.method === 'ping') return;
      const p = frame.params;
      if (!['subscribe','unsubscribe'].includes(frame.method) || !p) throw Error('Kraken Spot unsupported socket message');
      const privateChannel = ['executions','balances'].includes(p.channel);
      if (privateChannel !== privateSocket || !['executions','balances','book','ticker','ohlc'].includes(p.channel)) throw Error('Kraken Spot wrong channel endpoint');
      if (privateChannel) {
        if (!tokenState.token || p.token !== tokenState.token) throw Error('Kraken Spot subscription before delivered token');
        if (!socket.spotTokenRequest) {
          if (tokenState.tokenRequests <= (tokenState.usedTokenRequests ?? 0)) throw Error('Kraken Spot reconnect requires fresh token');
          socket.spotTokenRequest = tokenState.tokenRequests;
          tokenState.usedTokenRequests = tokenState.tokenRequests;
        }
      }
      const keys = privateChannel ? [p.channel] : (p.symbol ?? []).map((symbol) => `${p.channel}:${symbol}`);
      if (!keys.length || keys.some((key) => typeof key !== 'string')) throw Error('Kraken Spot missing subscription identity');
      for (const key of keys) if (frame.method === 'subscribe') own.topics.add(key); else own.topics.delete(key);
    },
    received(_url, text, socket) {
      const own = state(socket), frame = JSON.parse(text);
      if (own.closed) { failures.push('Kraken Spot data on closed socket'); return; }
      if (['heartbeat','status'].includes(frame.channel) || ['subscribe','unsubscribe','pong'].includes(frame.method)) return;
      const channel = frame.channel;
      if (['executions','balances'].includes(channel)) {
        if (!own.topics.has(channel)) failures.push('Kraken Spot private data without own subscription');
      } else if (['ticker','book','ohlc'].includes(channel)) {
        for (const row of frame.data ?? []) if (!own.topics.has(`${channel}:${row.symbol}`)) failures.push('Kraken Spot public data without own subscription');
      } else failures.push('Kraken Spot unknown business channel');
    },
    ready: () => !isPrivate || [...states.values()].some((s) => !s.closed && ['executions','balances'].every((channel) => s.topics.has(channel))),
    errors: () => [...new Set(failures)],
  };
}
