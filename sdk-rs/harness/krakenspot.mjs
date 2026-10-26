// Independent Spot protocol oracle: official REST auth and WS v2 contracts.
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
    if (u.origin !== 'https://api.kraken.com') throw Error('Kraken Spot request must use upstream host');
    const entries = (Array.isArray(headers) ? headers : Object.entries(headers)).map(([k,v]) => [k.toLowerCase(), String(v)]);
    if (new Set(entries.map(([k]) => k)).size !== entries.length) throw Error('Kraken Spot duplicate header');
    const h = Object.fromEntries(entries);
    if (u.pathname.startsWith('/0/public/')) {
      if (method !== 'GET' || h['api-key'] || h['api-sign']) throw Error('Kraken Spot public credential leak or method');
      return;
    }
    if (!u.pathname.startsWith('/0/private/') || method !== 'POST' || typeof body !== 'string')
      throw Error('Kraken Spot private request must use encoded POST');
    const form = new URLSearchParams(body), nonce = form.get('nonce');
    if (new Set(form.keys()).size !== [...form.keys()].length) throw Error('Kraken Spot duplicate parameter');
    if (h['api-key'] !== KRAKEN_SPOT_REPLAY.key || !/^\d+$/.test(nonce ?? '')) throw Error('Kraken Spot missing authentication');
    const n = BigInt(nonce);
    if (n <= previous || Math.abs(Number(n / 1000n) - now) > 60_000) throw Error('Kraken Spot nonce must be fresh and increasing');
    previous = n;
    if (h['api-sign'] !== krakenSpotSignature(u.pathname, body)) throw Error('Kraken Spot signature must cover exact form and path');
    if (u.pathname.endsWith('/GetWebSocketsToken')) state.tokenRequests = (state.tokenRequests ?? 0) + 1;
    if (u.pathname.endsWith('/AddOrder')) {
      const id = form.get('cl_ord_id');
      if (!/^[0-9a-f]{32}$/.test(id ?? '') || minted.has(id) || form.has('userref'))
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
export const krakenSpotRequestBody = (body) => {
  const result = { ...body };
  delete result.nonce;
  if (result.cl_ord_id !== undefined) result.cl_ord_id = '<generated>';
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
let audit, state;
export const krakenspot = {
  rust: 'krakenspot',
  virtualPrivateClock: true,
  recordAfterReady: true,
  ts: { module: '../../packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts', className: 'KrakenSpotExchange', name: 'krakenspot' },
  replayCredentials: KRAKEN_SPOT_REPLAY,
  privateSocket: 'ws-auth.kraken.com',
  requestBodies: true,
  compareWriteOrder: true,
  compareFillPrices: true,
  compareBalanceCurrencies: true,
  // Frames retain numeric currencies; TS-only raw response metadata is opaque.
  tsBalanceCurrencies: (currencies) => Object.fromEntries(Object.entries(currencies).map(([asset, row]) =>
    [asset, { free: row.free, used: row.used, total: row.total, upnl: row.upnl }])),
  initializeReplay() { state = {}; audit = createKrakenSpotRequestAudit(state); },
  validateRequest(request) { audit(request); },
  validateSocketMessage: (socket, text, now) => validateKrakenSpotSocketMessage(socket, text, now, state),
  validateSocketFrame: validateKrakenSpotSocketFrame,
  requestBody: krakenSpotRequestBody,
  commandResult: krakenSpotCommandResult,
  isWrite: (method, url) => method !== 'GET' && !/\/(Balance|OpenOrders|GetWebSocketsToken)$/.test(new URL(url).pathname),
  // start() creates its final public socket only after the ticker seed. Wait
  // for that socket's open event too: registering before open queues a 100 ms
  // retry as well as onOpen's resubscribe, making workload depend on auditing
  // startup time. Both replay modes register after the same open boundary.
  tsBooksReady: (exchange, store) => Boolean(store.loaded.tickers && exchange.publicWebsocket?.isConnected),
  tsBooksSubscribed: (exchange, count) => Object.values(exchange.publicWebsocket?.orderBookHandlers ?? {}).filter((v) => v.length).length >= count,
};
