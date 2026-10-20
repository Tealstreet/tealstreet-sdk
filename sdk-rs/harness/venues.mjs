// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac, createPrivateKey, createPublicKey, verify } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { subscriptionKeys } from './requests.mjs';
import { Signature, Wallet, parseUnits, verifyTypedData } from 'ethers';

const root = '../../packages/safe-cex/src/exchanges';

// Replay transport and signing clocks both default to wall-clock milliseconds.
const validWeexTimestamp = (time, now) => typeof time === 'string' && /^\d+$/.test(time)
  && Number.isSafeInteger(Number(time)) && Number(time) > 0
  && Number.isFinite(now) && Math.abs(Number(time) - now) <= 30_000;

// Public deterministic replay credentials, never a funded account.
const backpackSeed = Buffer.alloc(32, 7);
const backpackPrivate = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), backpackSeed]),
  format: 'der',
  type: 'pkcs8',
});
const backpackPublic = createPublicKey(backpackPrivate);
const backpackKey = backpackPublic.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
const backpackInstructions = {
  'GET /api/v1/account': 'accountQuery',
  'PATCH /api/v1/account': 'accountUpdate',
  'GET /api/v1/position': 'positionQuery',
  'GET /api/v1/capital/collateral': 'collateralQuery',
  'GET /api/v1/orders': 'orderQueryAll',
  'POST /api/v1/order': 'orderExecute',
  'POST /api/v1/orders': 'orderExecute',
  'DELETE /api/v1/order': 'orderCancel',
  'DELETE /api/v1/orders': 'orderCancelAll',
  'GET /wapi/v1/history/fills': 'fillHistoryQueryAll',
  'GET /wapi/v1/history/orders': 'orderHistoryQueryAll',
  'GET /wapi/v1/history/funding': 'fundingHistoryQueryAll',
};
const validBackpackTime = (timestamp, window, now) => /^\d+$/.test(String(timestamp ?? ''))
  && /^\d+$/.test(String(window ?? '')) && Number.isSafeInteger(Number(timestamp)) && Number(timestamp) > 0
  && Number.isSafeInteger(Number(window)) && Number(window) > 0 && Number(window) <= 60000
  && Math.abs(Number(timestamp) - now) <= Number(window);
const verifyBackpackSubscribe = (message, now = Date.now()) => {
  if (!(message.params ?? []).some((p) => p.startsWith('account.')) || message.method !== 'SUBSCRIBE') return true;
  const [key, signature, timestamp, window] = message.signature ?? [];
  return (
    key === backpackKey &&
    window === '5000' && validBackpackTime(timestamp, window, now) &&
    verify(
      null,
      Buffer.from(`instruction=subscribe&timestamp=${timestamp}&window=${window}`),
      backpackPublic,
      Buffer.from(signature ?? '', 'base64'),
    )
  );
};
const verifyBackpack = ({ method, url, headers, body, now = Date.now() }) => {
  const parsed = new URL(url);
  const instruction = backpackInstructions[`${method} ${parsed.pathname}`];
  if (!instruction) return true;
  const h = Object.fromEntries(
    (Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]),
  );
  if (h['x-api-key'] !== backpackKey || h['x-broker-id'] !== '1200'
    || !validBackpackTime(h['x-timestamp'], h['x-window'], now)) return false;
  const params = method === 'GET' ? Object.fromEntries(parsed.searchParams) : parseRequestJson(body ?? '{}');
  const rows = Array.isArray(params) ? params : [params];
  const pieces = rows.map(
    (row) =>
      `instruction=${instruction}` +
      Object.keys(row)
        .sort()
        .filter((k) => row[k] != null)
        .map((k) => `&${k}=${encodeURIComponent(String(row[k]))}`)
        .join(''),
  );
  const payload = `${pieces.join('&')}&timestamp=${h['x-timestamp']}&window=${h['x-window']}`;
  return verify(null, Buffer.from(payload), backpackPublic, Buffer.from(h['x-signature'] ?? '', 'base64'));
};

const okx = (name) => ({
  rust: name,
  ts: {
    module: `${root}/okx/${name.startsWith('okxeu') ? 'okxeu' : 'okx'}.exchange.ts`,
    className: name.startsWith('okxeu') ? 'OKXEUExchange' : 'OKXExchange',
    name,
  },
  options: { testnet: name.endsWith('demo') },
  configureTs: (exchange) => {
    // The Rust lane covers linear stablecoin swaps, including on the EEA class.
    exchange.getInstrumentTypes = () => ['SWAP'];
    exchange.isSupportedMarketInstrument = (m) =>
      m.instType === 'SWAP' && m.ctType === 'linear' && ['USDT', 'USDC'].includes(m.settleCcy);
  },
  tsBooksSubscribed: (exchange, count) =>
    Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('books.')).length >= count,
  isPrivateRecord: (record) => {
    if (record.type !== 'ws') return false;
    const path = new URL(record.url).pathname;
    if (path.endsWith('/private')) return true;
    if (!path.endsWith('/business')) return false;
    try {
      const message = JSON.parse(record.text);
      return message.event === 'login' || ['orders', 'orders-algo', 'algo-advance'].includes(message.arg?.channel);
    } catch {
      return false;
    }
  },
  matchRequestBody: true,
  isWrite: (method) => method !== 'GET',
  // Caller IDs remain exact, even when the caller chooses a broker-prefixed ID.
  generatedBodyId: (key, value) =>
    ['clOrdId', 'algoClOrdId'].includes(key) && typeof value === 'string' && value.startsWith('e847386590ce4dBC'),
  respondToPing: true,
  validateLogin: true,
  compareFillPrices: true,
});

// MEXC accepts snowflake IDs as either JSON numbers or strings. Keep every
// economic field's JSON type and every caller-provided client ID exact.
export const mexcRequestBody = (body, { url = '' } = {}) => {
  const ids = new Set(['id', 'orderId', 'positionId']);
  const visit = (value, key = '') => {
    if (ids.has(key) || (key === '<root-id>' && /\/private\/order\/cancel$/.test(new URL(url).pathname))) {
      if (JSON.isRawJSON(value)) return value.rawJSON;
      if (typeof value === 'number' && Number.isSafeInteger(value)) return String(value);
    }
    if (JSON.isRawJSON(value)) return value;
    if (Array.isArray(value)) return value.map((item) => visit(item, key === '' ? '<root-id>' : key));
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, visit(v, k)]));
    return value;
  };
  return visit(body);
};
export const mexcRequestUrl = (url) => {
  const parsed = new URL(url);
  const params = parsed.searchParams;
  if (parsed.hostname === 'api.mexc.com' && /^\/api\/v1\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname)
    && (params.get('states') === '1' || params.get('is_finished') === '0')
    && /^\d+$/.test(params.get('start_time') ?? '') && /^\d+$/.test(params.get('end_time') ?? '')) {
    params.set('start_time', String(BigInt(params.get('start_time')) - BigInt(params.get('end_time'))));
    params.set('end_time', 'now');
  }
  return parsed.href;
};

export const VENUES = {
  grvt: (() => {
    // Public deterministic replay key. Never use this well-known key for funds.
    const secret = '0x' + '01'.repeat(32);
    const signer = new Wallet(secret).address;
    const account = 'fixture-account';
    const builder = '0xee8f3559f298fffa79b8253437969d15b5dccff3';
    const instruments = {
      BTC_USDT_Perp: { hash: '0x030501', decimals: 9 },
      ETH_USDT_Perp: { hash: '0x030502', decimals: 9 },
    };
    // This schema is independent of the venue encoder and the GRVT SDK signer.
    const types = {
      OrderWithBuilderFee: [
        { name: 'subAccountID', type: 'uint64' }, { name: 'isMarket', type: 'bool' },
        { name: 'timeInForce', type: 'uint8' }, { name: 'postOnly', type: 'bool' },
        { name: 'reduceOnly', type: 'bool' }, { name: 'legs', type: 'OrderLeg[]' },
        { name: 'builder', type: 'address' }, { name: 'builderFee', type: 'uint32' },
        { name: 'nonce', type: 'uint32' }, { name: 'expiration', type: 'int64' },
      ],
      OrderLeg: [
        { name: 'assetID', type: 'uint256' }, { name: 'contractSize', type: 'uint64' },
        { name: 'limitPrice', type: 'uint64' }, { name: 'isBuyingContract', type: 'bool' },
      ],
    };
    const attempts = new Map();
    return {
      rust: 'grvt',
      ts: { module: `${root}/grvt/grvt.exchange.ts`, className: 'GrvtExchange', name: 'grvt' },
      tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((k) => k.startsWith('orderbook.')).length >= count,
      options: { grvtSessionProxy: true, grvtTradingAccountId: '777' },
      replayCredentials: { key: 'grvt-replay-key', secret },
      marketFilter: (row) => row.symbol.endsWith('_PERP'),
      requestBodies: true,
      generatedId: () => false,
      compareClientIds: true,
      compareFillPrices: true,
      compareSubscriptions: true,
      compareWriteOrder: true,
      isPrivateRecord: (record) => record.type === 'ws' && new URL(record.url).hostname.startsWith('trades.'),
      privateSocket: 'wss://trades.grvt.io/ws',
      isWrite: (_method, url) => /\/(create_order|cancel_order|cancel_all_orders|set_initial_leverage|set_position_config)$/.test(new URL(url).pathname),
      prepareTs(exchange) {
        // The production private stream requires a proxy-selected host. Supply
        // only that host boundary, using the recorded real upstream identity;
        // leave its first-frame auth, subscriptions and handlers untouched.
        exchange.options = { ...exchange.options, transport: {
          name: 'grvt-tape-routing',
          rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }),
          restAdapter: () => undefined, proxyPrefix: () => '', publicCorsProxy: () => undefined,
          privateProxy: () => undefined, serverProxyUrl: () => undefined,
          requireProxy: (options) => options, wsConnection: () => undefined,
          wsRouting: () => ({ restProxyHost: '', howEgress: 'sg',
            wsPublic: { tunnelled: false }, wsBusiness: { tunnelled: false },
            wsPrivate: { tunnelled: false, host: 'wss://trades.grvt.io/ws' },
          }),
        } };
      },
      requestBody(body, { url }) {
        if (!new URL(url).pathname.endsWith('/create_order') || !body?.o) return body;
        const order = body.o;
        // Only independently verified signature entropy and clock values vary.
        // Preserve field presence, signer, chain ID, caller/native IDs and fees.
        return { ...body, o: { ...order,
          s: Object.fromEntries(Object.entries(order.s ?? {}).map(([key, value]) => [key,
            ['r', 's1', 'v', 'e', 'n'].includes(key) ? `<${key}>` : value])),
          m: Object.fromEntries(Object.entries(order.m ?? {}).map(([key, value]) => [key, key === 'ct' ? '<ct>' : value])),
        } };
      },
      validateSocket(url) {
        const parsed = new URL(url);
        if (parsed.protocol !== 'wss:' || !['market-data.grvt.io', 'trades.grvt.io'].includes(parsed.hostname))
          throw new Error('GRVT must emit a real upstream socket URL');
      },
      validateRequest({ method, url, body, headers }) {
        const parsed = new URL(url);
        if (method !== 'POST' || parsed.protocol !== 'https:' || !['market-data.grvt.io', 'trades.grvt.io', 'edge.grvt.io'].includes(parsed.hostname))
          throw new Error('GRVT must emit a real upstream REST URL');
        const h = Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]));
        if (parsed.hostname === 'market-data.grvt.io') {
          if (['cookie', 'x-ts-grvt-session', 'x-grvt-account-id', 'x-api-key'].some((key) => h[key]))
            throw new Error('GRVT public market data must not carry private credentials');
          return;
        }
        if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
        const data = typeof body === 'string' ? parseRequestJson(body) : body;
        if (parsed.hostname === 'edge.grvt.io') {
          if (parsed.pathname !== '/auth/api_key/login' || data?.api_key !== 'grvt-replay-key' || h['x-grvt-remember-me'] !== 'true')
            throw new Error('Invalid GRVT login credentials');
          return;
        }
        const session = h['x-ts-grvt-session'];
        if (!['fixture-session-1', 'fixture-session-2'].includes(session) || h['x-grvt-account-id'] !== account
          || (h.cookie !== undefined && !h.cookie.split(';').map((v) => v.trim()).includes(`gravity=${session}`)))
          throw new Error('Invalid GRVT private session credentials');
        if (!parsed.pathname.endsWith('/create_order')) return;
        const o = data?.o, sig = o?.s;
        if (!o || !sig || !Array.isArray(o.l) || o.l.length !== 1 || !o.m || o.sa !== '777'
          || typeof o.im !== 'boolean' || typeof o.po !== 'boolean' || typeof o.ro !== 'boolean'
          || !['GOOD_TILL_TIME', 'ALL_OR_NONE', 'IMMEDIATE_OR_CANCEL', 'FILL_OR_KILL'].includes(o.ti)
          || sig.ci !== '325' || sig.s?.toLowerCase() !== signer.toLowerCase()
          || ![27, 28].includes(sig.v) || !Number.isInteger(sig.n) || sig.n < 0 || sig.n > 0xffff_ffff
          || !/^\d+$/.test(sig.e) || !/^\d+$/.test(o.m.ct)
          || !/^[1-9]\d*$/.test(o.m.co) || BigInt(o.m.co) <= 0n || BigInt(o.m.co) > 0x7fff_ffff_ffff_ffffn
          || typeof o.b !== 'string' || o.b.toLowerCase() !== builder || o.bf !== '0.03')
          throw new Error('Invalid GRVT signed order fields');
        const created = BigInt(o.m.ct), expires = BigInt(sig.e), now = BigInt(Date.now()) * 1_000_000n;
        if (created < now - 30_000_000_000n || created > now + 30_000_000_000n
          || expires < now + 3_570_000_000_000n || expires > now + 3_630_000_000_000n)
          throw new Error('Invalid GRVT signature clock');
        const message = {
          subAccountID: o.sa, isMarket: o.im,
          timeInForce: { GOOD_TILL_TIME: 1, ALL_OR_NONE: 2, IMMEDIATE_OR_CANCEL: 3, FILL_OR_KILL: 4 }[o.ti],
          postOnly: o.po, reduceOnly: o.ro,
          legs: o.l.map((leg) => {
            const instrument = instruments[leg.i];
            if (!instrument || typeof leg.s !== 'string' || typeof leg.ib !== 'boolean'
              || (o.im ? leg.lp !== undefined : typeof leg.lp !== 'string')) throw new Error('Invalid GRVT signed leg');
            const size = parseUnits(leg.s, instrument.decimals), price = o.im ? 0n : parseUnits(leg.lp, 9);
            if (size <= 0n || size > 0xffff_ffff_ffff_ffffn || (o.im ? price !== 0n : price <= 0n) || price > 0xffff_ffff_ffff_ffffn)
              throw new Error('Invalid GRVT native integer units');
            return { assetID: instrument.hash, contractSize: size, limitPrice: price, isBuyingContract: leg.ib };
          }),
          builder: o.b, builderFee: parseUnits(o.bf, 4), nonce: sig.n, expiration: sig.e,
        };
        const recovered = verifyTypedData({ name: 'GRVT Exchange', version: '0', chainId: 325 }, types, message,
          Signature.from({ r: sig.r, s: sig.s1, v: sig.v }));
        if (recovered.toLowerCase() !== signer.toLowerCase()) throw new Error('Invalid GRVT EIP-712 signature');
        const serialized = typeof body === 'string' ? body : JSON.stringify(body);
        const previous = attempts.get(o.m.co);
        if (previous && previous.body !== serialized) throw new Error('GRVT auth resend changed the signed order');
        attempts.set(o.m.co, { body: serialized });
      },
      createSocketAudit: (_impl, { isPrivate, books }) => {
        const states = new Map(), failures = [];
        const state = (url) => {
          if (!states.has(url)) states.set(url, { authSent: false, ack: false, streams: new Set(), pings: 0, messages: 0, auths: 0 });
          return states.get(url);
        };
        const fail = (message) => failures.push(message);
        return {
          sent(url, text) {
            const current = state(url), privateSocket = new URL(url).hostname === 'trades.grvt.io';
            if (text === 'pong') {
              if (current.pings <= 0) fail('unexpected GRVT pong'); else current.pings--;
              return;
            }
            if (text === 'ping') return;
            const frame = JSON.parse(text);
            if (privateSocket && frame.type === 'auth') {
              const expected = current.auths === 0 ? 'fixture-session-1' : 'fixture-session-2';
              if ((current.auths === 0 ? current.messages !== 0 : !current.ack || current.auths !== 1)
                || frame.session_cookie !== `rm=true; gravity=${expected}`
                || frame.account_id !== account || frame.testnet !== false) fail('invalid GRVT first-frame auth');
              current.authSent = true;
              current.auths++;
              current.ack = false;
              current.streams.clear();
            } else if (privateSocket) {
              if (!current.ack || frame.method !== 'subscribe' || !['v1.order', 'v1.position', 'v1.fill'].includes(frame.stream)
                || JSON.stringify(frame.feed) !== '["777"]' || frame.is_full !== false)
                fail('GRVT private subscription preceded auth acknowledgement or changed selector');
              else current.streams.add(frame.stream);
            } else if (frame.method === 'subscribe') {
              for (const selector of frame.params?.selectors ?? []) current.streams.add(`${frame.params.stream}:${selector}`);
            } else if (frame.method === 'unsubscribe') {
              for (const selector of frame.params?.selectors ?? []) current.streams.delete(`${frame.params.stream}:${selector}`);
            }
            current.messages++;
          },
          received(url, text) {
            const current = state(url);
            if (text === 'ping') { current.pings++; return; }
            if (text === 'pong') return;
            const frame = JSON.parse(text), stream = frame.s ?? frame.stream;
            if (frame.type === 'auth_success') {
              if (!current.authSent) fail('GRVT auth ACK preceded credentials');
              current.ack = true;
            }
            if (new URL(url).hostname === 'trades.grvt.io' && stream && (frame.f ?? frame.feed)) {
              if (!current.ack || !current.streams.has(stream)) fail('unsubscribed GRVT private data: ' + stream);
              const feed = frame.f ?? frame.feed, selector = frame.s1 ?? frame.selector;
              if ((feed.sa !== undefined && feed.sa !== '777') || (selector && !selector.startsWith('777-')))
                fail('GRVT private data changed sub-account selector');
            }
            if (new URL(url).hostname === 'market-data.grvt.io' && stream && (frame.f ?? frame.feed)) {
              const instrument = (frame.f ?? frame.feed).i ?? (frame.f ?? frame.feed).instrument;
              if (![...current.streams].some((key) => key.startsWith(`${stream}:${instrument}@`)))
                fail('unsubscribed GRVT public data: ' + stream + ':' + instrument);
            }
          },
          ready() { return !isPrivate || [...states.values()].some((s) => s.authSent); },
          errors() {
            for (const [url, current] of states) {
              if (current.pings) fail('missing GRVT pong');
              if (new URL(url).hostname === 'trades.grvt.io' && isPrivate)
                for (const stream of ['v1.order', 'v1.position', 'v1.fill']) if (!current.streams.has(stream)) fail('missing GRVT private stream: ' + stream);
              if (new URL(url).hostname === 'market-data.grvt.io') for (const symbol of books) {
                const native = symbol.replace(/_PERP$/, '_Perp');
                if (!current.streams.has(`v1.book.s:${native}@500-50`)) fail('missing GRVT book subscription: ' + symbol);
              }
            }
            return failures;
          },
        };
      },
    };
  })(),
  bitrue: {
    rust: 'bitrue',
    bitrueProtocol: true,
    ts: { module: `${root}/bitrue/bitrue.exchange.ts`, className: 'BitrueExchange', name: 'bitrue' },
    privateSocket: '/stream?listenKey=',
    compareFillPrices: true,
    requestBodies: true,
    volatileBodyFields: true,
    // Creation mints IDs; cancellation must retain the exact native target.
    generatedId: () => false,
    requestOptions: (method, url) => ({
      generatedBodyId: method === 'POST' && url.pathname === '/fapi/v2/order'
        ? (key) => key === 'clientOrderId'
        : undefined,
    }),
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/listenKey'),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('_simple_depth_step0')).length >= count,
    validateRequest: ({ method, url, body, headers }) => {
      const parsed = new URL(url);
      if (!['fapi.bitrue.com', 'fapiws-auth.bitrue.com'].includes(parsed.hostname))
        throw new Error('Bitrue must emit a real upstream URL');
      const fields = Object.fromEntries(Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]));
      if (!fields['x-ch-apikey'] && method === 'GET' && parsed.hostname === 'fapi.bitrue.com' && parsed.pathname === '/fapi/v1/contracts') return;
      if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
      const timestamp = fields['x-ch-ts'];
      const payload = timestamp + method + parsed.pathname + parsed.search + (body ?? '');
      if (fields['x-ch-apikey'] !== 'replay-key' || !/^\d+$/.test(timestamp) ||
          Math.abs(Date.now() - Number(timestamp)) > 60_000 ||
          fields['x-ch-sign'] !== createHmac('sha256', 'replay-secret').update(payload).digest('hex'))
        throw new Error('Invalid Bitrue REST signature');
    },
    createSocketAudit: (_impl, { isPrivate, books }) => {
      const states = new Map();
      const failures = [];
      const state = (url) => {
        if (!states.has(url)) states.set(url, { channels: new Set(), pings: new Set() });
        return states.get(url);
      };
      return {
        received(url, text) {
          const frame = JSON.parse(text);
          const current = state(url);
          if (frame.ping !== undefined) current.pings.add(JSON.stringify({ pong: frame.ping }));
          else if (frame.event === 'ping') current.pings.add(JSON.stringify(frame.ts === undefined ? { event: 'pong' } : { event: 'pong', ts: frame.ts }));
          if (frame.channel && (frame.tick || frame.data) && frame.event_rep !== 'rep' && !current.channels.has(frame.channel))
            failures.push('data arrived without subscription: ' + frame.channel);
        },
        sent(url, text) {
          const current = state(url);
          if (text === 'pong') return;
          const frame = JSON.parse(text);
          if (frame.event === 'sub') current.channels.add(frame.params.channel);
          if (frame.event === 'unsub') current.channels.delete(frame.params.channel);
          if (frame.event === 'pong' || frame.pong !== undefined) {
            const expected = [...current.pings].find((raw) => {
              const ping = JSON.parse(raw);
              return Object.entries(ping).every(([key, value]) => frame[key] === value);
            });
            if (!expected) failures.push('unexpected Bitrue heartbeat reply');
            else current.pings.delete(expected);
          }
        },
        ready() {
          return !isPrivate || [...states].some(([url, s]) => url.startsWith('wss://fapiws.bitrue.com/stream?listenKey=') && s.channels.has('user_account_update'));
        },
        errors() {
          for (const [url, s] of states) {
            if (s.pings.size) failures.push('missing Bitrue heartbeat reply');
            if (url.includes('fmarket-ws')) for (const symbol of books) {
              const native = 'e_' + symbol.replace(/^E-/, '').replaceAll('-', '').toLowerCase();
              if (!s.channels.has(`market_${native}_simple_depth_step0`)) failures.push('missing Bitrue depth subscription: ' + symbol);
            }
          }
          return failures;
        },
      };
    },
  },
  bitget: {
    rust: 'bitget',
    ts: { module: `${root}/bitget/bitget.exchange.ts`, className: 'BitgetExchange', name: 'bitget' },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(([key, handlers]) => key.startsWith('orderBook_') && handlers.length).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  bitgetusdc: {
    rust: 'bitgetusdc',
    ts: { module: `${root}/bitget/bitgetusdc.exchange.ts`, className: 'BitgetUSDCExchange', name: 'bitgetusdc' },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(([key, handlers]) => key.startsWith('orderBook_') && handlers.length).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  backpack: {
    rust: 'backpack',
    ignoreGeneratedBodyId: (key, value, callerIds, context) => key === 'clientId' && !callerIds.has(String(value))
      && context?.method === 'POST' && /^\/api\/v1\/orders?$/.test(new URL(context.url).pathname)
      && typeof value === 'number' && Number.isSafeInteger(value) && value > 0 && value <= 0xffff_ffff,
    ts: { module: `${root}/backpack/backpack.exchange.ts`, className: 'BackpackExchange', name: 'backpack' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depth.')).length >= count,
    isPrivateRecord: (record) => record.type === 'ws' && JSON.parse(record.text).stream?.startsWith('account.'),
    streamOf: (text) => {
      try {
        return JSON.parse(text).stream;
      } catch {
        return undefined;
      }
    },
    marketFilter: (row) => row.symbol.endsWith('_PERP'),
    replayCredentials: { key: backpackKey, secret: backpackSeed.toString('base64') },
    verifyRequest: verifyBackpack,
    verifySubscribe: verifyBackpackSubscribe,
    compareSubscriptions: true,
    compareWriteOrder: true,
    compareClientIds: true,
    compareFillPrices: true,
    isWrite: (method) => method !== 'GET',
  },

  hyperliquid: {
    rust: 'hyperliquid',
    ts: { module: `${root}/hyperliquid/hyperliquid.exchange.ts`, className: 'HyperliquidExchange', name: 'hyperliquid' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
    privateSocket: undefined,
    publicOptions: { key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
  },
  blofin: {
    rust: 'blofin',
    blofinProtocol: true,
    validateSocketMessage(socket, text, now = Date.now()) {
      if (text === 'ping' || text === 'pong') return;
      const frame = JSON.parse(text);
      if (frame.op === 'subscribe' && new URL(socket.url).pathname === '/ws/private' && !socket.loginAcknowledged)
        throw new Error('BloFin subscriptions preceded valid login acknowledgement');
      if (frame.op !== 'login') return;
      const arg = frame.args?.length === 1 ? frame.args[0] : {};
      const { apiKey, passphrase, timestamp, nonce, sign } = arg ?? {};
      if (!validWeexTimestamp(String(timestamp), now) || apiKey !== 'replay-key' || passphrase !== 'replay-passphrase'
        || typeof nonce !== 'string' || !nonce.length) throw new Error('invalid BloFin socket login');
      const expected = Buffer.from(createHmac('sha256', 'replay-secret')
        .update(`/users/self/verifyGET${timestamp}${nonce}`).digest('hex')).toString('base64');
      if (sign !== expected) throw new Error('invalid BloFin socket signature');
      socket.loginVerified = true;
    },
    validateSocketFrame(socket, text) {
      if (text === 'pong') return;
      const frame = JSON.parse(text);
      if (frame.event === 'login' && String(frame.code) === '0') {
        if (!socket.loginVerified) throw new Error('BloFin login acknowledgement preceded valid login');
        socket.loginAcknowledged = true;
      }
    },
    requestUrl(url) {
      const parsed = new URL(url);
      if (parsed.pathname === '/api/v1/market/instruments') {
        for (const [key, value] of parsed.searchParams) if (value === '') parsed.searchParams.delete(key);
      }
      return parsed.href;
    },
    ts: { module: `${root}/blofin/blofin.exchange.ts`, className: 'BlofinExchange', name: 'blofin' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('books.')).length >= count,
    privateSocket: '/ws/private',
    compareFillPrices: true,
    options: { passphrase: 'replay-passphrase' },
    cacheBustPaths: ['/api/v1/market/instruments'],
    requestBodies: true,
    volatileBodyFields: true,
    isWrite: (method) => method !== 'GET',
    defaultBodyValues: { tpTriggerPriceType: 'last', slTriggerPriceType: 'last', triggerPriceType: 'last' },
    generatedId: (key, value) => key === 'clientOrderId' && value.startsWith('b3cdedc0a20880ba'),
  },
  mexc: {
    rust: 'mexc',
    mexcProtocol: true,
    requestBody: mexcRequestBody,
    requestUrl: mexcRequestUrl,
    generatedBodyId: (key) => key === 'externalOid',
    validateSocketMessage(socket, text, now = Date.now()) {
      const frame = JSON.parse(text);
      if (frame.method !== 'login') return;
      const { apiKey, reqTime, signature } = frame.param ?? {};
      if (!validWeexTimestamp(reqTime, now) || apiKey !== 'replay-key'
        || signature !== createHmac('sha256', 'replay-secret').update(`${apiKey}${reqTime}`).digest('hex')) {
        throw new Error('invalid MEXC socket login');
      }
      socket.loginVerified = true;
    },
    validateSocketFrame(socket, text) {
      const frame = JSON.parse(text);
      if (frame.channel === 'rs.login' && frame.data === 'success' && !socket.loginVerified) throw new Error('MEXC login acknowledgement preceded valid login');
    },
    matchRequestBody: true,
    ts: { module: `${root}/mexc/mexc.exchange.ts`, className: 'MexcExchange', name: 'mexc' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depth.')).length >= count,
    // Public/private sockets share this URL; tapes identify their ordinal.
    privateSocket: undefined,
    independentLeverage: true,
    tsSettings: (exchange, store) =>
      store.markets.map((market) => {
        const config = exchange.symbolSettings.get(market.symbol) ?? exchange.deriveSymbolConfig(market.symbol);
        return {
          symbol: market.symbol,
          marginType: config.marginType,
          buyLeverage: config.buyLeverage,
          sellLeverage: config.sellLeverage,
        };
      }),
  },
  bingx: {
    rust: 'bingx',
    ts: { module: `${root}/bingx/bingx.exchange.ts`, className: 'BingxExchange', name: 'bingx' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('market.depth.')).length >=
      count,
    privateSocket: '?listenKey=',
    matchRequestBody: true,
    bingxProtocol: true,
    // BingX owns one listener per URL; resubscriptions must recur on reconnect.
    subscriptionGenerations: true,
  },
  weex: {
    rust: 'weex',
    ts: { module: `${root}/weex/weex.exchange.ts`, className: 'WeexExchange', name: 'weex' },
    privateSocket: '/v3/ws/private',
    matchRequestBody: true,
    volatileBodyFields: true,
    // WEEX's income history is a read despite using POST.
    isWrite: (method, url) =>
      method !== 'GET' && !(method === 'POST' && new URL(url).pathname === '/capi/v3/account/income'),
    options: { passphrase: 'replay-passphrase', privateWsHeaders: true },
    tsBooksSubscribed: (exchange, count) =>
      [...exchange.publicWebsocket.topics].filter((t) => t.endsWith('@depth200')).length >= count,
    prepareTs(exchange, WebSocket) {
      for (const socket of [exchange.publicWebsocket, exchange.privateWebsocket]) {
        socket.createWebsocket = (url, protocols, options = {}) => {
          const headers = typeof options.headers === 'function' ? options.headers() : options.headers;
          const ws = new WebSocket(url, protocols, { headers });
          ws.addEventListener('open', socket.trackConnected);
          return ws;
        };
      }
    },
    validateRequest({ method, url, body = '', headers = {}, now = Date.now() }) {
      const parsed = new URL(url);
      if (parsed.pathname.startsWith('/capi/v3/market/')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      if (!validWeexTimestamp(time, now)) throw new Error('invalid WEEX authentication timestamp');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}${method}${parsed.pathname}${parsed.search}${body || ''}`).digest('base64');
      if (lookup('ACCESS-KEY') !== 'replay-key' || lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' || lookup('ACCESS-SIGN') !== signature) throw new Error(`invalid WEEX signed request: ${method} ${parsed.pathname}`);
    },
    validateSocket(url, headers = {}, now = Date.now()) {
      if (!url.includes('/v3/ws/private')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      if (!validWeexTimestamp(time, now)) throw new Error('invalid WEEX authentication timestamp');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}/v3/ws/private`).digest('base64');
      if (
        lookup('ACCESS-KEY') !== 'replay-key' ||
        lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' ||
        lookup('ACCESS-SIGN') !== signature
      )
        throw new Error('invalid WEEX upgrade authentication');
    },
  },
  cryptocom: {
    rust: 'cryptocom',
    ts: { module: root + '/cryptocom/cryptocom.exchange.ts', className: 'CryptocomExchange', name: 'cryptocom' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('book.')).length >= count,
    privateSocket: '/exchange/v1/user',
    matchRequestBody: true,
    cryptoProtocol: true,
    // Authentication is validated separately; economic params stay exact.
    requestBody: (body) => body?.method && body?.params ? body.params : body,
    publicTickers: (records) => {
      const seed = records.find((r) => r.type === 'http' && r.url.endsWith('/public/get-instruments'));
      return seed
        ? JSON.parse(seed.body)
            .result.data.filter((r) => r.inst_type === 'PERPETUAL_SWAP' && r.tradable)
            .map((r) => r.symbol)
        : [];
    },
    // KNOWN_TS_FIXES: public-only startup returns before market subscriptions.
    prepareTsReady: (exchange, { isPrivate }) => {
      if (!isPrivate)
        exchange.publicWebsocket.subscribeToMarketData(
          exchange.store.markets.map((m) => ({ instrumentName: m.id, indexInstrumentName: m.info?.underlyingSymbol })),
        );
    },
    // KNOWN_TS_FIXES: the global ticker/book throttles drop updates across
    // symbols. Reset after every frame so each next update gets a leading call,
    // without retaining trailing timers during accelerated replay.
    afterTsFrame: (exchange) => {
      exchange.publicWebsocket.handleTickerUpdate.cancel();
      exchange.publicWebsocket.handleBookUpdate.cancel();
    },
    isWrite: (method, url) =>
      method === 'POST' && /private\/(advanced\/)?(create|amend|cancel|change|close)-/.test(new URL(url).pathname),
    createSocketAudit: (impl, { isPrivate, books, tickers = [] }) => {
      const connections = new Map();
      const failures = [];
      const socket = (url) => {
        if (!connections.has(url))
          connections.set(url, { auth: false, authAck: false, channels: new Set(), heartbeats: new Set() });
        return connections.get(url);
      };
      return {
        received(url, text) {
          // Market data has no auth/heartbeat state to audit.
          if (!text.includes('public/heartbeat') && !text.includes('public/auth')) return;
          const frame = JSON.parse(text);
          if (frame.method === 'public/heartbeat') socket(url).heartbeats.add(frame.id);
          if (frame.method === 'public/auth' && frame.code === 0) {
            if (!socket(url).auth) failures.push('auth acknowledgement preceded valid auth request');
            socket(url).authAck = socket(url).auth;
          }
        },
        sent(url, text) {
          const frame = JSON.parse(text);
          const state = socket(url);
          if (frame.method === 'public/auth') {
            const payload = frame.method + frame.id + frame.api_key + frame.nonce;
            state.auth =
              Number.isSafeInteger(frame.id) &&
              Number.isSafeInteger(frame.nonce) &&
              Math.abs(Date.now() - frame.nonce) < 60_000 &&
              frame.api_key === 'replay-key' &&
              frame.sig === createHmac('sha256', 'replay-secret').update(payload).digest('hex');
            if (!state.auth) failures.push('invalid private WS auth signature');
          } else if (frame.method === 'subscribe') {
            if (url.endsWith('/user') && !state.authAck)
              failures.push('private subscriptions sent before authentication');
            for (const channel of frame.params.channels) state.channels.add(channel);
          } else if (frame.method === 'unsubscribe') {
            for (const channel of frame.params.channels) state.channels.delete(channel);
          } else if (frame.method === 'public/respond-heartbeat') {
            if (!state.heartbeats.delete(frame.id)) failures.push('unexpected heartbeat response id');
          } else {
            failures.push('unexpected outbound WS method ' + frame.method);
          }
        },
        ready() {
          const entries = [...connections.entries()];
          if (isPrivate && !entries.some(([url, state]) => url.endsWith('/user') && state.auth)) return false;
          if (
            tickers.length &&
            !entries.some(
              ([url, state]) =>
                url.endsWith('/market') &&
                tickers.slice(0, 350 - books.length).every((symbol) => state.channels.has('ticker.' + symbol)),
            )
          )
            return false;
          if (
            books.length &&
            !entries.some(
              ([url, state]) =>
                url.endsWith('/market') && books.every((symbol) => state.channels.has('book.' + symbol + '.50')),
            )
          )
            return false;
          return true;
        },
        errors() {
          for (const [url, state] of connections) {
            if (state.heartbeats.size) failures.push('heartbeat response missing');
            if (url.endsWith('/user') && isPrivate) {
              if (!state.auth) failures.push('private WS never authenticated');
              const required = [
                'user.order',
                'user.advance.order',
                'user.positions',
                'user.balance',
                ...(impl === 'rust' ? ['user.trade'] : []),
              ];
              for (const channel of required)
                if (!state.channels.has(channel)) failures.push('missing private subscription ' + channel);
            }
            if (url.endsWith('/market'))
              for (const symbol of tickers.slice(0, 350 - books.length)) {
                if (!state.channels.has('ticker.' + symbol)) failures.push('missing ticker subscription ' + symbol);
              }
            if (url.endsWith('/market'))
              for (const symbol of books) {
                if (!state.channels.has('book.' + symbol + '.50')) failures.push('missing book subscription ' + symbol);
              }
          }
          return failures;
        },
      };
    },
    checkRequest: (method, url, body) => {
      const path = new URL(url).pathname.replace('/exchange/v1/', '');
      if (!path.startsWith('private/')) return true;
      try {
        const envelope = parseRequestJson(body);
        const params = Object.keys(envelope.params)
          .sort()
          .map((key) => {
            const value = envelope.params[key];
            return key + (Array.isArray(value) ? value.join('') : String(value));
          })
          .join('');
        const payload = envelope.method + envelope.id + envelope.api_key + params + envelope.nonce;
        return (
          method === 'POST' &&
          envelope.method === path &&
          Number.isSafeInteger(envelope.id) &&
          Number.isSafeInteger(envelope.nonce) &&
          Math.abs(Date.now() - envelope.nonce) < 60_000 &&
          envelope.api_key === 'replay-key' &&
          createHmac('sha256', 'replay-secret').update(payload).digest('hex') === envelope.sig
        );
      } catch {
        return false;
      }
    },
  },
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >=
      count,
    privateSocket: '/private/ws',
    volatileBodyFields: true,
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
  },
  bybit: {
    rust: 'bybit',
    ts: { module: `${root}/bybit/bybit.exchange.ts`, className: 'BybitExchange', name: 'bybit' },
    /** Book topics get a message handler once each subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderbook.')).length >=
      count,
    startupSubscriptions: (records, books) => {
      const symbols = new Set();
      for (const record of records) {
        if (record.type !== 'http' || record.method !== 'GET' || !(record.status >= 200 && record.status < 300)) continue;
        const url = new URL(record.url);
        if (url.pathname !== '/v5/market/instruments-info' || (url.searchParams.has('category') && url.searchParams.get('category') !== 'linear')) continue;
        const body = JSON.parse(record.body);
        if (Number(body.retCode) !== 0) continue;
        for (const row of body.result?.list ?? []) {
          if (['USDT', 'USDC'].includes(row.quoteCoin) && typeof row.symbol === 'string' && row.symbol)
            symbols.add(row.symbol);
        }
      }
      const socket = records.find((record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/public/linear')?.url
        ?? 'wss://stream.bybit.com/v5/public/linear';
      return subscriptionKeys(socket, { op: 'subscribe', args: [...symbols].map((symbol) => `tickers.${symbol}`)
        .concat(books.map((symbol) => `orderbook.1000.${symbol}`)) });
    },
    privateSocket: '/v5/private',
    tsRejectReason: (order) =>
      order.status === 'canceled' && order.info?.rejectReason === 'EC_PostOnlyWillTakeLiquidity'
        ? 'PostOnly'
        : undefined,
    isWrite: (method) => method === 'POST',
    matchRequestBody: true,
    volatileBodyFields: true,
  },
  ...Object.fromEntries(['okx', 'okxeu', 'okxdemo', 'okxeudemo'].map((name) => [name, okx(name)])),
};

export const venueOf = (name) => {
  const venue = VENUES[name];
  if (!venue) throw new Error(`unknown venue ${name}; known: ${Object.keys(VENUES).join(', ')}`);
  return venue;
};

/** Every axios instance the exchange owns, so all of its REST goes to the tape. */
export const axiosClients = (exchange) =>
  [...new Set(Object.values(exchange))].filter((value) => value?.defaults && value?.interceptors);
