// Public deterministic test keys. JWT verification uses Node crypto, independent
// of both the venue's Rust signer and the frozen TS implementation.
import { uniqueRequestHeaders } from './protocol-headers.mjs';
import { createPrivateKey, createPublicKey, verify, createECDH } from 'node:crypto';

const seed = Buffer.alloc(32, 13);
const edPrivate = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
  format: 'der', type: 'pkcs8',
});
const ecSecret = Buffer.alloc(32, 7);
const ec = createECDH('prime256v1');
ec.setPrivateKey(ecSecret);
const ecPrivate = createPrivateKey({
  key: Buffer.concat([
    Buffer.from('30770201010420', 'hex'), ecSecret,
    Buffer.from('a00a06082a8648ce3d030107a144034200', 'hex'), ec.getPublicKey(),
  ]), format: 'der', type: 'sec1',
});
export const COINBASE_REPLAY = {
  key: 'organizations/replay-org/apiKeys/replay-key',
  secret: seed.toString('base64'),
};
export const COINBASE_ES256_REPLAY = {
  key: COINBASE_REPLAY.key,
  secret: ecPrivate.export({ format: 'pem', type: 'sec1' }).toString(),
};
const keys = { EdDSA: createPublicKey(edPrivate), ES256: createPublicKey(ecPrivate) };
const canonicalPart = (part) => typeof part === 'string' && /^[A-Za-z0-9_-]+$/.test(part)
  && Buffer.from(part, 'base64url').toString('base64url') === part;

export const verifyCoinbaseJwt = (token, { method, path, now = Date.now(), algorithm } = {}) => {
  try {
    if (typeof token !== 'string' || token.length > 8192 || !Number.isFinite(now)) return false;
    const parts = token.split('.');
    if (parts.length !== 3 || !parts.every(canonicalPart)) return false;
    const header = JSON.parse(Buffer.from(parts[0], 'base64url'));
    const claims = JSON.parse(Buffer.from(parts[1], 'base64url'));
    if (!Object.hasOwn(keys, header.alg) || (algorithm && header.alg !== algorithm)
      || header.typ !== 'JWT' || header.kid !== COINBASE_REPLAY.key
      || typeof header.nonce !== 'string' || header.nonce.length < 10 || header.nonce.length > 128 || /[^A-Za-z0-9_-]/.test(header.nonce)
      || claims.sub !== COINBASE_REPLAY.key
      || claims.iss !== (header.alg === 'ES256' ? 'coinbase-cloud' : 'cdp')
      || (header.alg === 'EdDSA' ? JSON.stringify(claims.aud) !== '["cdp_service"]' : claims.aud !== undefined)
      || !Number.isSafeInteger(claims.nbf) || !Number.isSafeInteger(claims.exp)
      || claims.exp - claims.nbf !== 120 || claims.nbf < 0
      || Math.abs(Math.floor(now / 1000) - claims.nbf) > 5) return false;
    if (method) {
      if (claims.uri !== `${method} api.coinbase.com${path.split('?')[0]}`) return false;
    } else if (claims.uri !== undefined) return false;
    const signature = Buffer.from(parts[2], 'base64url');
    return signature.length === 64 && verify(header.alg === 'ES256' ? 'sha256' : null,
      Buffer.from(`${parts[0]}.${parts[1]}`),
      header.alg === 'ES256' ? { key: keys.ES256, dsaEncoding: 'ieee-p1363' } : keys.EdDSA, signature);
  } catch { return false; }
};

export const verifyCoinbaseRequest = ({ method, url, headers, now = Date.now() }) => {
  try {
    if (typeof url !== 'string' || /[\x00-\x20\x7f]/.test(url)) return false;
    const h = uniqueRequestHeaders(headers), parsed = new URL(url);
    if (parsed.protocol !== 'https:' || parsed.username || parsed.password || parsed.hash || parsed.port) return false;
    const auth = ['authorization', 'coinbase-authorization'].filter((key) => Object.hasOwn(h, key));
    if (auth.length > 1) return false;
    if (parsed.hostname === 'api.international.coinbase.com') {
      return auth.length === 0 && method === 'GET' && ['/api/v1/instruments', '/api/v1/instruments/'].includes(parsed.pathname);
    }
    if (parsed.hostname !== 'api.coinbase.com') return false;
    if (auth.length) {
      const token = h[auth[0]];
      return token.startsWith('Bearer ') && verifyCoinbaseJwt(token.slice(7), { method, path: parsed.pathname, now });
    }
    return method === 'GET' && (/^\/api\/v3\/brokerage\/market\/products(?:\/[^/]+(?:\/(?:candles|ticker))?)?$/.test(parsed.pathname)
      || parsed.pathname === '/api/v3/brokerage/market/product_book');
  } catch { return false; }
};

export const verifyCoinbaseSubscribe = (message) => {
  if (!Object.hasOwn(message, 'jwt')) return !['user'].includes(message.channel);
  return message.type === 'subscribe' && ['user', 'heartbeats'].includes(message.channel)
    && verifyCoinbaseJwt(message.jwt);
};

export const coinbaseSubscription = (message) => {
  if (!Object.hasOwn(message, 'jwt')) return message;
  return { ...message, jwt: '<verified-jwt>' };
};

export const createCoinbaseSocketAudit = (_impl, { isPrivate, books = [] }) => {
  const connections = new Map(), errors = [], tokens = new Set();
  const get = (url, socket = 1) => {
    const key = typeof socket === 'object' && socket !== null ? socket : `${url} ${socket}`;
    if (!connections.has(key)) {
      const parsed = new URL(url), privateSocket = parsed.hostname === 'advanced-trade-ws-user.coinbase.com';
      const valid = parsed.protocol === 'wss:' && !parsed.port && !parsed.username && !parsed.password && !parsed.search && !parsed.hash
        && parsed.pathname === '/' && ['advanced-trade-ws.coinbase.com', 'advanced-trade-ws-user.coinbase.com'].includes(parsed.hostname)
        && (socket?.channel === undefined || socket.channel === (privateSocket ? 'private' : 'public'));
      if (!valid) errors.push('Coinbase invalid socket origin or role');
      connections.set(key, { private: privateSocket, channels: new Map(), closed: false, valid });
    }
    return connections.get(key);
  };
  const active = (url, socket) => {
    const state = get(url, socket);
    if (state.closed) errors.push('Coinbase traffic on closed socket');
    return state.valid && !state.closed ? state : undefined;
  };
  return {
    connected(url, socket) { get(url, socket); },
    closed(url, socket) { get(url, socket).closed = true; },
    sent(url, text, socket) {
      const state = active(url, socket); if (!state) return;
      let m;
      try { m = JSON.parse(text); } catch { errors.push('Coinbase sent non-JSON subscription'); return; }
      if (!['subscribe', 'unsubscribe'].includes(m?.type)) { errors.push('Coinbase sent unknown message type'); return; }
      if (state.private) {
        if (!verifyCoinbaseSubscribe(m) || typeof m.jwt !== 'string' || m.product_ids !== undefined) { errors.push('Coinbase private subscription JWT or scope invalid'); return; }
        if (tokens.has(m.jwt)) errors.push('Coinbase reused a subscription JWT');
        tokens.add(m.jwt);
      } else if (m.jwt !== undefined || !['heartbeats', 'ticker', 'ticker_batch', 'level2', 'candles', 'market_trades', 'status'].includes(m.channel)
        || !Array.isArray(m.product_ids) || m.product_ids.some((id) => typeof id !== 'string' || !id)) {
        errors.push('Coinbase invalid public subscription or private auth'); return;
      }
      const products = state.channels.get(m.channel) ?? new Set();
      if (m.type === 'subscribe') { for (const id of m.product_ids ?? []) products.add(id); state.channels.set(m.channel, products); }
      else if (m.product_ids?.length) { for (const id of m.product_ids) products.delete(id); }
      else state.channels.delete(m.channel);
    },
    received(url, text, socket) {
      const state = active(url, socket); if (!state) return;
      let m;
      try { m = JSON.parse(text); } catch { errors.push('Coinbase received invalid JSON'); return; }
      if (m?.type === 'error') { errors.push('Coinbase rejected a socket subscription'); return; }
      if (m?.channel === 'subscriptions') { if (!state.channels.size) errors.push('Coinbase unsolicited subscription acknowledgement'); return; }
      const channel = m?.channel === 'l2_data' ? 'level2' : m?.channel;
      if (!state.channels.has(channel)) { errors.push('Coinbase received unsubscribed channel'); return; }
      if (!state.private) for (const event of m.events ?? []) {
        const rows = [event, ...(event.tickers ?? []), ...(event.trades ?? [])];
        for (const row of rows) if (row.product_id !== undefined && !state.channels.get(channel).has(row.product_id)) errors.push('Coinbase received unsubscribed product');
      }
    },
    ready() {
      const states = [...connections.values()].filter((state) => state.valid && !state.closed);
      return states.some((state) => !state.private && state.channels.has('heartbeats')
        && books.every((symbol) => state.channels.get('level2')?.has(normalizeCoinbaseSymbol(symbol))))
        && (!isPrivate || states.some((state) => state.private && ['user', 'heartbeats'].every((channel) => state.channels.has(channel))));
    },
    errors() { return [...errors, ...(this.ready() ? [] : ['Coinbase missing required socket subscriptions'])]; },
  };
};

export const normalizeCoinbaseSymbol = (symbol) => symbol.endsWith('-USDC') && !['USDT-USDC', 'EURC-USDC'].includes(symbol)
  ? symbol.slice(0, -5) + '-USD' : symbol;
