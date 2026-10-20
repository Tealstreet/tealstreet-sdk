import { uniqueRequestHeaders } from './protocol-headers.mjs';
// DX REST login/session and single-order contracts, independent of Rust output.
export const BREAKOUT_REPLAY_TOKEN = 'documented-dx-session';
export const BREAKOUT_ACCOUNT = 'default:00002';
const validOrderCode = (value) => typeof value === 'string' && value.length > 11 && value.length <= 32 && value.startsWith('tealstreet-') && !/[^A-Za-z0-9_-]/.test(value);
export const verifyBreakoutRequest = ({ method, url, body = '', headers = {} }) => {
  if (typeof url !== 'string' || /[\x00-\x20\x7f]/.test(url)) throw new Error('Invalid DXtrade URL');
  const h = uniqueRequestHeaders(headers), parsed = new URL(url);
  if (parsed.origin !== 'https://breakout.prosp.devexperts.com' || parsed.username || parsed.password || parsed.hash || !parsed.pathname.startsWith('/dxsca-web/')) throw new Error('Invalid DXtrade origin');
  if (Object.hasOwn(h, 'authorization') && Object.hasOwn(h, 'coinbase-authorization')) throw new Error('Ambiguous DXtrade authentication');
  const path = parsed.pathname.slice('/dxsca-web'.length);
  const data = body ? JSON.parse(body) : undefined;
  if (path === '/login') {
    if (method !== 'POST' || JSON.stringify(Object.keys(data ?? {}).sort()) !== JSON.stringify(['domain', 'password', 'username']) || data.domain !== 'default' || data.username !== 'replay-key' || data.password !== 'replay-secret') throw new Error('Invalid DXtrade basic login');
    return;
  }
  const auth = h.authorization ?? h['coinbase-authorization'];
  if (auth !== `DXAPI ${BREAKOUT_REPLAY_TOKEN}`) throw new Error('Invalid DXtrade session authentication');
  const account = path.match(/^\/accounts\/([^/]+)(?:\/|$)/)?.[1];
  if ((account !== undefined && decodeURIComponent(account) !== BREAKOUT_ACCOUNT) || (parsed.searchParams.has('accountCode') && parsed.searchParams.get('accountCode') !== BREAKOUT_ACCOUNT)) throw new Error('DXtrade request selected a different account');
  if (path === '/marketdata' && data?.account !== BREAKOUT_ACCOUNT) throw new Error('DXtrade market data account missing');
  if (method === 'DELETE' && h['if-match'] !== '*') throw new Error('DXtrade conditional cancellation header missing');
  if (method === 'POST' && path.endsWith('/orders')) {
    const allowed = ['orderCode', 'type', 'instrument', 'quantity', 'side', 'limitPrice', 'stopPrice', 'positionEffect', 'tif'];
    if (Object.keys(data).some((key) => !allowed.includes(key)) || !validOrderCode(data.orderCode) || !['BUY', 'SELL'].includes(data.side) || !['OPEN', 'CLOSE'].includes(data.positionEffect) || data.tif !== 'GTC' || !Number.isFinite(data.quantity) || (data.quantity < 0 || (data.quantity === 0 && data.positionEffect !== 'CLOSE')) || typeof data.instrument !== 'string' || !data.instrument || /[\x00-\x1f\x7f]/.test(data.instrument)) throw new Error('Invalid DXtrade single order');
    if (!['LIMIT', 'MARKET', 'STOP'].includes(data.type)) throw new Error('Unsupported DXtrade order type');
    if ((data.type === 'LIMIT') !== ('limitPrice' in data) || (data.type === 'STOP') !== ('stopPrice' in data)) throw new Error('Invalid DXtrade order price fields');
    for (const key of ['limitPrice', 'stopPrice']) if (key in data && (!Number.isFinite(data[key]) || data[key] <= 0)) throw new Error('Invalid DXtrade order price');
  }
};

export const breakoutReplayOptions = () => ({ key: 'replay-key', secret: 'replay-secret', subaccount: '00002' });
// Sanitize the session plus account/user identity before a live record reaches disk.
const recordingStates = new WeakMap();
export const sanitizeBreakoutRecord = (record, options) => {
  let state = recordingStates.get(options);
  if (!state) {
    state = { accounts: new Map(), selected: options.subaccount ? (options.subaccount.includes(':') ? options.subaccount : `default:${options.subaccount}`) : undefined };
    recordingStates.set(options, state);
  }
  const account = (value) => {
    if (value === state.selected) return BREAKOUT_ACCOUNT;
    if (!state.accounts.has(value)) state.accounts.set(value, `default:other-${state.accounts.size + 1}`);
    return state.accounts.get(value);
  };
  const clean = (value) => {
    if (Array.isArray(value)) return value.map(clean);
    if (!value || typeof value !== 'object') return value;
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key,
      key === 'sessionToken' ? BREAKOUT_REPLAY_TOKEN :
      key === 'password' ? 'replay-secret' :
      key === 'account' && typeof item === 'string' ? account(item) :
      ['owner', 'username'].includes(key) && typeof item === 'string' ? (item.endsWith('@default') ? 'replay-key@default' : 'replay-key') :
      ['login', 'fullName'].includes(key) && typeof item === 'string' ? 'replay-key' : clean(item),
    ]));
  };
  const safe = { ...record };
  for (const field of ['body', 'requestBody']) {
    let data;
    try { data = JSON.parse(record[field]); } catch { continue; }
    if (data.userDetails) {
      const candidates = data.userDetails.filter((user) => user.login === options.key && user.domain === 'default').flatMap((user) => user.accounts).filter((row) => row.accountStatus === 'FULL_TRADING');
      if (candidates.length === 1) state.selected = candidates[0].account;
    }
    safe[field] = JSON.stringify(clean(data));
  }
  if (safe.url) {
    const url = new URL(safe.url);
    url.pathname = url.pathname.replace(/\/accounts\/([^/]+)/, (_, raw) => `/accounts/${encodeURIComponent(account(decodeURIComponent(raw)))}`);
    url.pathname = url.pathname.replace(/\/users\/[^/]+/, '/users/replay-key');
    if (url.searchParams.has('accountCode')) url.searchParams.set('accountCode', account(url.searchParams.get('accountCode')));
    safe.url = url.href;
  }
  return safe;
};

export const breakoutprop = {
  rust: 'breakoutprop',
  preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  generatedId: () => false,
  polling: true,
  recordSession: true,
  recordReplayOptions: breakoutReplayOptions,
  sanitizeRecordingRecord: sanitizeBreakoutRecord,
  validateRequest: verifyBreakoutRequest,
  options: { subaccount: '00002' },
  recordOptions: { subaccount: undefined },
  publicOptions: { key: 'replay-key', secret: 'replay-secret' },
  ts: { module: '../../packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.exchange.ts', className: 'BreakoutpropExchange', name: 'breakoutprop' },
  tsBooksSubscribed: (exchange, count) => Object.entries(exchange.publicWebsocket.messageHandlers).filter(([key, handlers]) => key.startsWith('poll_book.') && handlers.length).length >= count,
  requestUrl: (url) => { const parsed = new URL(url); parsed.pathname = parsed.pathname.split('/').map((part) => encodeURIComponent(decodeURIComponent(part))).join('/'); return parsed.href; },
  tsSettings: (_exchange, store) => store.positions.filter((p) => p.leverage > 0).map((p) => ({ symbol: p.symbol, leverage: p.leverage, marginType: p.marginType })).filter((row, i, rows) => rows.findIndex((other) => other.symbol === row.symbol) === i),
  matchRequestBody: true,
  requestBodies: true,
  compareWriteOrder: true,
  isWrite: (method, url) => method !== 'GET' && !['/dxsca-web/login', '/dxsca-web/marketdata'].includes(new URL(url).pathname),
  ignoreGeneratedBodyId: (key, value, callerIds, context) => key === 'orderCode' && !callerIds.has(value) && context?.method === 'POST' && new URL(context.url).pathname.endsWith('/orders') && validOrderCode(value),
};
