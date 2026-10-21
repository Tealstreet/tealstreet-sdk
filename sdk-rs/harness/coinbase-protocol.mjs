// Public deterministic test keys. JWT verification uses Node crypto, independent
// of both the venue's Rust signer and the frozen TS implementation.
import { createPrivateKey, createPublicKey, verify, createECDH, sign } from 'node:crypto';

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
      || typeof header.nonce !== 'string' || !/^[A-Za-z0-9_-]{10,128}$/.test(header.nonce)
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

// TS ES256 awaits WebCrypto's thread pool, which a virtual replay clock cannot
// order against tape frames or its timers. Sign the same imported P-256 key on
// the calling thread, so each request resolves in that turn's microtasks.
export const installSynchronousEs256 = (subtle = globalThis.crypto.subtle) => {
  const { importKey, sign: asyncSign } = subtle;
  const keys = new WeakMap();
  subtle.importKey = async function (format, data, algorithm, ...rest) {
    const key = await importKey.call(this, format, data, algorithm, ...rest);
    if (format === 'pkcs8' && algorithm?.name === 'ECDSA' && algorithm.namedCurve === 'P-256')
      keys.set(key, createPrivateKey({ key: Buffer.from(data), format: 'der', type: 'pkcs8' }));
    return key;
  };
  subtle.sign = function (algorithm, key, data) {
    if (!keys.has(key) || algorithm?.name !== 'ECDSA' || algorithm.hash !== 'SHA-256')
      return asyncSign.call(this, algorithm, key, data);
    const signature = sign('sha256', data, { key: keys.get(key), dsaEncoding: 'ieee-p1363' });
    return Promise.resolve(signature.buffer.slice(signature.byteOffset, signature.byteOffset + signature.length));
  };
};

export const verifyCoinbaseRequest = ({ method, url, headers, now = Date.now() }) => {
  const parsed = new URL(url);
  if (parsed.hostname === 'api.international.coinbase.com') return true;
  if (parsed.hostname !== 'api.coinbase.com') return false;
  if (parsed.pathname.startsWith('/api/v3/brokerage/market/')) return true;
  const entries = Array.isArray(headers) ? headers : Object.entries(headers ?? {});
  const values = entries.filter(([key]) => ['authorization', 'coinbase-authorization'].includes(key.toLowerCase()));
  if (values.length !== 1) return false;
  const token = String(values[0][1]);
  return token.startsWith('Bearer ') && verifyCoinbaseJwt(token.slice(7), {
    method, path: parsed.pathname, now,
  });
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

export const createCoinbaseSocketAudit = (_impl, { isPrivate, books }) => {
  const connections = new Map();
  const errors = [];
  const tokens = new Set();
  const get = (url, generation = 1) => {
    const key = `${url} ${generation}`;
    if (!connections.has(key)) connections.set(key, { private: url.includes('-ws-user.'), channels: new Set(), books: new Set(), closed: false });
    return connections.get(key);
  };
  return {
    connected(url, generation) { get(url, generation); },
    closed(url, generation) { get(url, generation).closed = true; },
    sent(url, text, generation) {
      let message;
      try { message = JSON.parse(text); } catch { errors.push('Coinbase sent non-JSON subscription'); return; }
      const state = get(url, generation);
      if (!['subscribe', 'unsubscribe'].includes(message.type)) { errors.push('Coinbase sent unknown message type'); return; }
      if (state.private) {
        if (!verifyCoinbaseSubscribe(message) || typeof message.jwt !== 'string'
          || message.product_ids !== undefined) errors.push('Coinbase private subscription JWT or scope invalid');
        if (tokens.has(message.jwt)) errors.push('Coinbase reused a subscription JWT');
        tokens.add(message.jwt);
      } else if (message.jwt !== undefined || message.channel === 'user') errors.push('Coinbase private auth sent on public socket');
      if (message.type === 'subscribe') {
        state.channels.add(message.channel);
        if (message.channel === 'level2') for (const symbol of message.product_ids ?? []) state.books.add(symbol);
      } else {
        state.channels.delete(message.channel);
        if (message.channel === 'level2') for (const symbol of message.product_ids ?? []) state.books.delete(symbol);
      }
    },
    received(_url, text) {
      try { if (JSON.parse(text).type === 'error') errors.push('Coinbase rejected a socket subscription'); }
      catch { errors.push('Coinbase received invalid JSON'); }
    },
    ready() {
      const active = [...connections.values()].filter((state) => !state.closed);
      return active.some((state) => !state.private && state.channels.has('heartbeats')
        && books.every((symbol) => state.books.has(normalizeCoinbaseSymbol(symbol))))
        && (!isPrivate || active.some((state) => state.private && ['user', 'heartbeats'].every((channel) => state.channels.has(channel))));
    },
    errors() { return [...errors, ...(this.ready() ? [] : ['Coinbase missing required socket subscriptions'])]; },
  };
};

export const normalizeCoinbaseSymbol = (symbol) => symbol.endsWith('-USDC') && !['USDT-USDC', 'EURC-USDC'].includes(symbol)
  ? symbol.slice(0, -5) + '-USD' : symbol;
