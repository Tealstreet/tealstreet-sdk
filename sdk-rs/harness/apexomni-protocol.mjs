// Test-only independent authentication oracle: real npm zkLink SDK, never Rust.
import { createHash, createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const BigNumber = require('bignumber.js');

export const APEX_SEED = '01'.repeat(64) + '1b';
export const APEX_ACCOUNT = '611779989544632629';
export const APEX_PAIR = 50000;
const MAKER = '0.0002';
const TAKER = '0.0005';
const HOUR = 3600000;
const API_KEY = 'replay-key';
const SECRET = 'replay-secret';
const PASSPHRASE = 'replay-passphrase';
const hosts = new Set(['omni.apex.exchange', 'testnet.omni.apex.exchange']);
const wsHosts = new Set(['quote.omni.apex.exchange', 'qa-quote.omni.apex.exchange']);
let sdk;
let signer;
let privateReplay = false;
const orderExpirations = new Map();

export async function prepareApexReplay({ isPrivate } = {}) {
  privateReplay = Boolean(isPrivate);
  if (!isPrivate || sdk) return;
  // Import the same package entry used by TS. Its SDK is only a replay oracle;
  // native engine signatures are independently checked against these bytes.
  sdk = await import('zklink-web-sdk-alpha');
  const init = typeof sdk.default === 'function' ? sdk.default : sdk.default.default;
  await init(readFileSync(require.resolve('zklink-web-sdk-alpha/zklink-sdk-web_bg.wasm')));
  signer = sdk.newRpcSignerWithProvider({});
  await signer.initZklinkSigner(APEX_SEED);
}

export async function prepareApexTs(exchange) {
  if (exchange.isPublicDataOnly) return;
  await prepareApexReplay({ isPrivate: true });
  const { initSdk, sdk: tsSdk } = await import('../../packages/safe-cex/src/__tests__/reference/apexomni/zklink/signer/utils.ts');
  // tsx's CJS view and this ESM oracle can own separate package instances.
  const init = typeof tsSdk.default === 'function' ? tsSdk.default : tsSdk.default.default;
  await init(readFileSync(require.resolve('zklink-web-sdk-alpha/zklink-sdk-web_bg.wasm')));
  await initSdk();
}

const encode = (value) => encodeURIComponent(String(value)).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
const canonical = (values) => Object.entries(values).filter(([, value]) => value !== undefined && value !== null)
  .sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) => `${encode(key)}=${encode(value)}`).join('&');
const headersOf = (headers) => Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers ?? {}))
  .map(([key, value]) => [key.toLowerCase(), String(value)]));
const hmac = (secret, text) => createHmac('sha256', Buffer.from(secret).toString('base64')).update(text).digest('base64');
const check = (condition, message) => { if (!condition) throw new Error(`ApeX Omni: ${message}`); };

export function apexWireBody(body) {
  if (body === undefined || body === null || body === '') return undefined;
  if (typeof body?.getBuffer === 'function') body = body.getBuffer().toString('utf8');
  else if (body?.entries && typeof body !== 'string' && !ArrayBuffer.isView(body)) {
    return Object.fromEntries([...body.entries()].map(([key, value]) => [key, String(value)]));
  }
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  if (typeof body === 'string' && body.startsWith('--')) {
    const firstLine = body.indexOf('\r\n');
    check(firstLine > 2, 'multipart boundary is missing');
    const boundary = body.slice(0, firstLine);
    const parts = body.split(boundary);
    check(parts.at(-1) === '--\r\n' || parts.at(-1) === '--', 'multipart terminator is missing');
    const fields = {};
    for (const part of parts.slice(1, -1)) {
      const match = /^\r\nContent-Disposition: form-data; name="([^"\r\n]+)"\r\n\r\n([\s\S]*)\r\n$/.exec(part);
      check(Boolean(match), 'invalid multipart part');
      check(!(match[1] in fields), 'duplicate multipart field');
      fields[match[1]] = match[2];
    }
    return fields;
  }
  if (typeof body === 'string') {
    if (body.trimStart().startsWith('{')) body = JSON.parse(body);
    else {
      const fields = {};
      for (const [key, value] of new URLSearchParams(body)) {
        check(!(key in fields), 'duplicate form field');
        fields[key] = value;
      }
      return fields;
    }
  }
  check(body && typeof body === 'object' && !Array.isArray(body), 'unexpected body shape');
  return Object.fromEntries(Object.entries(body).map(([key, value]) => [key, String(value)]));
}

function signedContract(id, size, price, buy) {
  check(Boolean(sdk && signer), 'SDK oracle was not initialized');
  const hash = new BigNumber(createHash('sha256').update(id).digest('hex'), 16);
  const contract = sdk.newContract(new sdk.ContractBuilder(
    new BigNumber(APEX_ACCOUNT).mod('4294967295').toNumber(), 0,
    hash.mod('18446744073709551615').div('4294967295').toNumber(),
    hash.mod('4294967295').toNumber(), APEX_PAIR,
    new BigNumber(size).times('1000000000000000000').toFixed(0, BigNumber.ROUND_DOWN),
    new BigNumber(price).times('1000000000000000000').toFixed(0, BigNumber.ROUND_DOWN),
    buy, Math.abs(Math.ceil(Number(MAKER) * 10000)), Math.ceil(Number(TAKER) * 10000), false,
  ));
  return signer.createSignedContract(contract).signature.signature;
}
export const apexSignatureForTest = signedContract;
const fee = (price, size) => {
  const value = new BigNumber(price).times(TAKER).times(size).toNumber();
  return (Math.ceil(new BigNumber(value).times(1e6).toNumber()) / 1e6).toFixed(6);
};
function checkId(id, timestamp, callerIds) {
  check(typeof id === 'string' && /^\S+$/.test(id), 'invalid client order ID');
  if (!callerIds?.has(id)) {
    const match = new RegExp(`^apexomni-${APEX_ACCOUNT}-(\\d+)-[1-9]\\d{5}$`).exec(id);
    check(match && Math.abs(Number(match[1]) - timestamp) < 60000, 'invalid generated client order ID');
  }
}
function validateOrder(fields, timestamp, callerIds) {
  check(fields.symbol === 'BTC-USDT', 'unexpected signed order symbol');
  check(fields.brokerId === '6587', 'broker ID drift');
  check(fields.clientId === fields.clientOrderId, 'client IDs differ');
  check(['BUY', 'SELL'].includes(fields.side), 'invalid order side');
  check(['LIMIT', 'MARKET', 'STOP_MARKET', 'STOP_LIMIT', 'TAKE_PROFIT_MARKET', 'TAKE_PROFIT_LIMIT'].includes(fields.type), 'invalid order type');
  check(['GOOD_TIL_CANCEL', 'IMMEDIATE_OR_CANCEL', 'FILL_OR_KILL', 'POST_ONLY'].includes(fields.timeInForce), 'invalid time in force');
  check(Number(fields.size) > 0 && Number(fields.price) > 0, 'invalid signed quantity or price');
  checkId(fields.clientId, timestamp, callerIds);
  const expirationAt = (time) => (Math.ceil((time + 30 * 86400000) / HOUR) + 168) * HOUR;
  const expiration = Number(fields.expiration);
  const originalExpiration = orderExpirations.get(fields.clientId);
  // Legacy auth discards subsecond precision; its original order remains intact.
  const expirations = originalExpiration === undefined
    ? [expirationAt(timestamp), ...(timestamp % 1000 === 0 ? [expirationAt(timestamp + 999)] : [])]
    : [originalExpiration];
  check(expirations.includes(expiration), 'invalid parent expiration');
  check(fields.limitFee === fee(fields.price, fields.size), 'invalid parent limit fee');
  check(fields.signature === signedContract(fields.clientId, fields.size, fields.price, fields.side === 'BUY'), 'invalid parent L2 signature');
  const ids = new Set([fields.clientId]);
  check(fields.isOpenTpslOrder !== 'true' || fields.isSetOpenTp === 'true' || fields.isSetOpenSl === 'true', 'bracket flag has no leg');
  for (const [prefix, flag] of [['tp', 'isSetOpenTp'], ['sl', 'isSetOpenSl']]) {
    if (fields[flag] !== 'true') {
      check(!Object.keys(fields).some((key) => key.startsWith(prefix)), 'unflagged bracket fields');
      continue;
    }
    check(fields.isOpenTpslOrder === 'true', 'missing parent bracket flag');
    const id = fields[`${prefix}ClientOrderId`];
    checkId(id, timestamp);
    check(!ids.has(id), 'bracket reuses a client ID'); ids.add(id);
    check(fields[`${prefix}Size`] === fields.size, 'bracket size differs from parent');
    check(fields[`${prefix}Side`] === (fields.side === 'BUY' ? 'SELL' : 'BUY'), 'bracket direction is not opposite');
    check(Number(fields[`${prefix}Expiration`]) === expiration, 'invalid bracket expiration');
    check(Number(fields[`${prefix}TriggerPrice`]) > 0, 'invalid bracket trigger');
    check(['MARKET', 'INDEX'].includes(fields[`${prefix}TriggerPriceType`]), 'invalid bracket trigger type');
    check(fields[`${prefix}LimitFee`] === fee(fields[`${prefix}Price`], fields.size), 'invalid bracket limit fee');
    check(fields[`${prefix}Signature`] === signedContract(id, fields.size, fields[`${prefix}Price`], fields.side !== 'BUY'), 'invalid bracket L2 signature');
  }
  orderExpirations.set(fields.clientId, expiration);
}

export function validateApexRequest({ method, url, body, headers = {}, now = Date.now(), callerIds }) {
  const parsed = new URL(url);
  check(hosts.has(parsed.hostname) && parsed.protocol === 'https:', 'request must use the real upstream URL');
  check(parsed.pathname.startsWith('/api/v3/'), 'unexpected API path');
  const h = headersOf(headers);
  if (parsed.pathname === '/api/v3/klines' && method === 'GET') {
    check(!Object.keys(h).some((key) => key.startsWith('apex-')), 'klines must be unauthenticated');
    return;
  }
  // Public symbols still uses the TS interceptor, but has no account identity.
  if (parsed.pathname === '/api/v3/symbols' && !privateReplay) return;
  const timestamp = h['apex-timestamp'];
  check(/^\d+$/.test(timestamp ?? '') && Number.isSafeInteger(Number(timestamp)) && Math.abs(Number(timestamp) - now) < 60000, 'invalid authentication timestamp');
  check(h['apex-api-key'] === API_KEY, 'invalid API key');
  check(h['apex-passphrase'] === PASSPHRASE, 'invalid passphrase');
  const fields = apexWireBody(body);
  const query = Object.fromEntries(parsed.searchParams);
  const queryMethod = method === 'GET' || method === 'DELETE';
  const serialized = queryMethod ? canonical(query) : typeof body === 'string' && !body.startsWith('--') ? body : canonical(fields ?? {});
  const message = `${timestamp}${method}${parsed.pathname}${queryMethod && serialized ? '?' : ''}${serialized}`;
  const messages = [message];
  if (queryMethod && Number(timestamp) % 1000 === 0) {
    const legacy = { ...query };
    for (const key of ['beginTimeInclusive', 'endTimeExclusive']) if (legacy[key] === '0') delete legacy[key];
    const serialized = canonical(legacy);
    messages.push(`${timestamp}${method}${parsed.pathname}${serialized ? '?' : ''}${serialized}`);
  }
  check(messages.some((message) => h['apex-signature'] === hmac(SECRET, message)), 'invalid REST HMAC');
  if (method !== 'GET' && method !== 'DELETE') {
    if (parsed.pathname === '/api/v3/set-initial-margin-rate') {
      check(h['content-type']?.startsWith('multipart/form-data'), 'leverage is not multipart');
      if (typeof body === 'string') {
        const boundary = /(?:^|;)\s*boundary=(?:"([^"]+)"|([^;]+))/.exec(h['content-type']);
        check(boundary && body.startsWith(`--${boundary[1] ?? boundary[2]}\r\n`), 'multipart header boundary differs from body');
      }
      check(fields.symbol === 'BTC-USDT' && fields.token === 'USDT' && Number(fields.initialMarginRate) > 0, 'invalid leverage fields');
    } else check(h['content-type']?.startsWith('application/x-www-form-urlencoded'), 'write is not form encoded');
  }
  if (method === 'POST' && parsed.pathname === '/api/v3/order') validateOrder(fields, Number(timestamp), callerIds);
}

export function createApexSocketAudit(_impl, { isPrivate, books = [] }) {
  const states = new Map();
  const failures = [];
  const state = (url) => {
    if (!states.has(url)) states.set(url, { private: new URL(url).pathname === '/realtime_private', auth: false, topics: new Set(), pings: 0, pending: [] });
    return states.get(url);
  };
  return {
    connected(url) {
      const parsed = new URL(url);
      if (!wsHosts.has(parsed.hostname) || parsed.protocol !== 'wss:' || !['/realtime_public', '/realtime_private'].includes(parsed.pathname) || parsed.searchParams.get('v') !== '2') failures.push('socket must use the real upstream URL and v2');
      states.delete(url); state(url);
    },
    sent(url, text) {
      const s = state(url);
      const message = JSON.parse(text);
      if (message.op === 'login') {
        let login;
        try { check(message.args?.length === 1 && typeof message.args[0] === 'string', 'login args must contain one JSON string'); login = JSON.parse(message.args[0]); }
        catch { failures.push('invalid nested login payload'); return; }
        const valid = s.private && Object.keys(login).sort().join(',') === 'apiKey,httpMethod,passphrase,requestPath,signature,timestamp,type'
          && login.type === 'login' && login.httpMethod === 'GET' && login.requestPath === '/ws/accounts'
          && login.apiKey === API_KEY && login.passphrase === PASSPHRASE && Number.isSafeInteger(login.timestamp)
          && Math.abs(login.timestamp - Date.now()) < 60000 && login.signature === hmac(SECRET, `${login.timestamp}GET/ws/accounts`);
        if (!valid) failures.push('invalid private login signature or shape');
        s.auth = valid;
      } else if (message.op === 'subscribe' || message.op === 'unsubscribe') {
        if (s.private && !s.auth) failures.push('private subscription preceded login');
        if (!Array.isArray(message.args) || !message.args.length || message.args.some((topic) => typeof topic !== 'string' || !topic)) { failures.push('invalid subscription topic'); return; }
        for (const topic of message.args) if (message.op === 'subscribe') s.topics.add(topic); else s.topics.delete(topic);
      } else if (message.op === 'ping') {
        if (s.private && !s.auth) failures.push('private heartbeat preceded login');
        s.pings++;
      } else if (message.op === 'pong') {
        const expected = s.pending.shift();
        if (!expected) failures.push('unexpected server ping response');
        else if (expected.args !== undefined && expected.args !== JSON.stringify(message.args)) failures.push('server ping arguments were not echoed');
        else if (expected.args === undefined && !(Array.isArray(message.args) && message.args.length === 1
          && typeof message.args[0] === 'string' && /^\d+$/.test(message.args[0]) && Number.isSafeInteger(Number(message.args[0]))
          && Math.abs(Number(message.args[0]) - Date.now()) < 60000)) failures.push('server ping fallback timestamp is invalid');
      } else failures.push('unexpected outbound socket operation');
    },
    received(url, text) {
      const s = state(url);
      const message = JSON.parse(text);
      if (message.op === 'ping' || (s.private && 'ping' in message)) s.pending.push({ args: s.private && message.args?.length ? JSON.stringify(message.args) : undefined });
      if (message.topic && !s.topics.has(message.topic)) failures.push(`frame arrived without subscription ${message.topic}`);
      if (s.private && message.topic && !s.auth) failures.push('private data preceded login');
    },
    ready() {
      return [...states.values()].some((s) => !s.private && ['instrumentInfo.all', ...books.map((symbol) => `orderBook200.H.${symbol}`)].every((topic) => s.topics.has(topic)))
        && (!isPrivate || [...states.values()].some((s) => s.private && s.auth && ['ws_zk_accounts_v3', 'ws_user_v1', 'ws_notify_v1'].every((topic) => s.topics.has(topic))));
    },
    errors() {
      for (const s of states.values()) {
        if (!s.pings) failures.push('socket has no heartbeat');
        if (s.pending.length) failures.push('server ping was not answered');
        if (s.private && (!s.auth || !['ws_zk_accounts_v3', 'ws_user_v1', 'ws_notify_v1'].every((topic) => s.topics.has(topic)))) failures.push('private socket did not restore authentication and subscriptions');
      }
      return [...new Set(failures)];
    },
  };
}
