// Engine-supported environments, not a catalogue of every venue's offerings.
// Keep this allowlist aligned with venues/* and registry/src/lib.rs. Never
// accept an endpoint from an environment variable or an API response unchecked.
import { VENUES } from './venues.mjs';

const profiles = {};
const register = (names, family, environments, fields = { key: 'KEY', secret: 'SECRET' }, extra = {}) => {
  for (const name of names.split(' ')) profiles[name] = { family, environments, fields, ...extra };
};
const endpoints = (http, ws) => ({ http: http.split(' '), ws: ws.split(' ') });
const modes = (testnet, mainnet) => ({ testnet, mainnet });
register('binance', 'BINANCE', modes(
  endpoints('https://demo-fapi.binance.com', 'wss://demo-fstream.binance.com'),
  endpoints('https://fapi.binance.com', 'wss://fstream.binance.com'),
));
// Spot testnet keys are separate from futures demo keys, so Spot has its own env family.
register('binancespot', 'BINANCESPOT', modes(
  endpoints('https://testnet.binance.vision', 'wss://stream.testnet.binance.vision wss://ws-api.testnet.binance.vision'),
  endpoints('https://api.binance.com', 'wss://stream.binance.com:9443 wss://ws-api.binance.com'),
));
for (const name of ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo']) {
  const testnet = endpoints('https://api-testnet.bybit.com', 'wss://stream-testnet.bybit.com');
  const demo = endpoints('https://api-demo.bybit.com', 'wss://stream.bybit.com wss://stream-demo.bybit.com');
  register(name, 'BYBIT', name === 'bybitdemo' ? { testnet, demo } : {
    testnet, mainnet: endpoints('https://api.bybit.com', 'wss://stream.bybit.com'),
  }, undefined, name === 'bybitdemo' ? { defaultMode: 'demo' } : {});
}
register('bitget bitgetusdc bitgetinverse', 'BITGET', modes(
  endpoints('https://api.bitget.com', 'wss://ws.bitget.com'),
  endpoints('https://api.bitget.com', 'wss://ws.bitget.com'),
), { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE' });
register('orderly tealstreet-dex kodiakfi mode salsadex what woofi', 'ORDERLY', modes(
  endpoints('https://testnet-api-evm.orderly.org', 'wss://testnet-ws-evm.orderly.org wss://testnet-ws-private-evm.orderly.org'),
  endpoints('https://api-evm.orderly.org', 'wss://ws-evm.orderly.org wss://ws-private-evm.orderly.org'),
), { key: 'KEY', secret: 'SECRET', applicationId: 'ACCOUNT_ID' });
register('lighter', 'LIGHTER', modes(
  endpoints('https://testnet-api.lighter.xyz', 'wss://testnet.zklighter.elliot.ai'),
  endpoints('https://mainnet.zklighter.elliot.ai', 'wss://mainnet.zklighter.elliot.ai'),
), { secret: 'PRIVATE_KEY', applicationId: 'ACCOUNT_INDEX', apiKeyIndex: 'API_KEY_INDEX' });
register('phemex', 'PHEMEX', modes(
  endpoints('https://testnet-api.phemex.com', 'wss://testnet-api.phemex.com'),
  endpoints('https://api.phemex.com', 'wss://ws.phemex.com'),
));
register('cryptocom', 'CRYPTOCOM', modes(
  endpoints('https://uat-api.3ona.co', 'wss://uat-stream.3ona.co'),
  endpoints('https://api.crypto.com', 'wss://stream.crypto.com'),
));
register('hyperliquid hyperliquidspot', 'HYPERLIQUID', modes(
  endpoints('https://api.hyperliquid-testnet.xyz', 'wss://api.hyperliquid-testnet.xyz'),
  endpoints('https://api.hyperliquid.xyz', 'wss://api-ui.hyperliquid.xyz'),
), { key: 'SIGNER_ADDRESS', secret: 'PRIVATE_KEY', applicationId: 'ACCOUNT_ADDRESS' }, {
  optionalFields: { vault: 'VAULT_ADDRESS' },
});
register('apexomni', 'APEXOMNI', modes(
  endpoints('https://testnet.omni.apex.exchange', 'wss://qa-quote.omni.apex.exchange'),
  endpoints('https://omni.apex.exchange', 'wss://quote.omni.apex.exchange'),
), { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE', omniKeySeed: 'OMNI_KEY_SEED' });
register('asterdex', 'ASTERDEX', modes(
  endpoints('https://fapi.asterdex-testnet.com', 'wss://fstream5.asterdex-testnet.com'),
  endpoints('https://fapi.asterdex.com', 'wss://fstream.asterdex.com'),
), { key: 'KEY', secret: 'SECRET' }, {
  optionalFields: {
    'asterdex.apiVersion': 'API_VERSION', 'asterdex.userAddress': 'USER_ADDRESS',
    'asterdex.signerAddress': 'SIGNER_ADDRESS', 'asterdex.privateKey': 'PRIVATE_KEY',
  },
});
register('paradex', 'PARADEX', modes(
  endpoints('https://api.testnet.paradex.trade', 'wss://ws.api.testnet.paradex.trade'),
  endpoints('https://api.prod.paradex.trade', 'wss://ws.api.prod.paradex.trade'),
), { key: 'PRIVATE_KEY', paradexAddress: 'ACCOUNT_ADDRESS', subkeyAddress: 'SUBKEY_ADDRESS' });
register('woo', 'WOO', modes(
  endpoints('https://api.staging.woox.io https://api-pub.woox.io', 'wss://wss.staging.woox.io'),
  endpoints('https://api.woox.io https://api-pub.woox.io', 'wss://wss.woox.io'),
), { key: 'KEY', secret: 'SECRET', applicationId: 'APPLICATION_ID' });
register('blofin', 'BLOFIN', {
  demo: endpoints('https://demo-trading-openapi.blofin.com', 'wss://demo-trading-openapi.blofin.com'),
  mainnet: endpoints('https://openapi.blofin.com', 'wss://openapi.blofin.com'),
}, { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE' });
for (const name of ['okx', 'okxeu', 'okxdemo', 'okxeudemo']) {
  const eea = name.startsWith('okxeu');
  const demo = endpoints(eea ? 'https://eea.okx.com' : 'https://openapi.okx.com',
    eea ? 'wss://wseeapap.okx.com' : 'wss://wspap.okx.com');
  // EEA accounts and keys are separate from global OKX ones.
  register(name, eea ? 'OKX_EEA' : 'OKX', {
    demo,
    ...(!name.endsWith('demo') ? { mainnet: endpoints(eea ? 'https://eea.okx.com' : 'https://www.okx.com',
      eea ? 'wss://wseea.okx.com' : 'wss://ws.okx.com') } : {}),
  }, { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE' });
}
register('kraken', 'KRAKEN', {
  demo: endpoints('https://demo-futures.kraken.com', 'wss://demo-futures.kraken.com'),
  mainnet: endpoints('https://futures.kraken.com', 'wss://futures.kraken.com'),
});
register('krakenspot', 'KRAKEN_SPOT', {
  mainnet: endpoints('https://api.kraken.com', 'wss://ws.kraken.com wss://ws-auth.kraken.com'),
});
register('coinbase coinbasespot', 'COINBASE', {
  mainnet: endpoints('https://api.coinbase.com https://api.international.coinbase.com',
    'wss://advanced-trade-ws.coinbase.com wss://advanced-trade-ws-user.coinbase.com'),
});
register('backpack', 'BACKPACK', {
  mainnet: endpoints('https://api.backpack.exchange', 'wss://ws.backpack.exchange'),
});
register('mexc', 'MEXC', {
  mainnet: endpoints('https://api.mexc.com', 'wss://contract.mexc.com'),
});
register('bingx', 'BINGX', {
  mainnet: endpoints('https://open-api.bingx.com', 'wss://ws-market-swap.qq-os.com wss://open-api-swap.bingx.com'),
});
register('weex', 'WEEX', {
  mainnet: endpoints('https://api-contract.weex.com', 'wss://ws-contract.weex.com'),
}, { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE' }, { options: { privateWsHeaders: true } });
register('bitrue', 'BITRUE', {
  mainnet: endpoints('https://fapi.bitrue.com https://fapiws-auth.bitrue.com',
    'wss://fmarket-ws.bitrue.com wss://fapiws.bitrue.com'),
});
register('bitunix', 'BITUNIX', {
  mainnet: endpoints('https://fapi.bitunix.com',
    'wss://fapi.bitunix.com wss://api.bitunix.com'),
});
register('kucoin', 'KUCOIN', {
  mainnet: endpoints('https://api-futures.kucoin.com', 'wss://ws-api-futures.kucoin.com'),
}, { key: 'KEY', secret: 'SECRET', passphrase: 'PASSPHRASE' });
register('polymarket', 'POLYMARKET', {
  mainnet: endpoints('https://clob.polymarket.com https://gamma-api.polymarket.com https://data-api.polymarket.com',
    'wss://ws-subscriptions-clob.polymarket.com'),
}, { key: 'PRIVATE_KEY' }, {
  optionalFields: {
    'polymarket.apiKey': 'API_KEY', 'polymarket.apiSecret': 'API_SECRET',
    'polymarket.apiPassphrase': 'API_PASSPHRASE', 'polymarket.funderAddress': 'FUNDER_ADDRESS',
    'polymarket.signerType': 'SIGNER_TYPE', 'polymarket.marketSlugs': 'MARKET_SLUGS',
  },
});

export const LIVE_SMOKE_VENUES = Object.freeze(Object.keys(VENUES));

const setOption = (options, path, value) => {
  const parts = path.split('.');
  let target = options;
  for (const part of parts.slice(0, -1)) target = target[part] ??= {};
  target[parts.at(-1)] = value;
};

const testMode = (profile) => {
  const availableModes = Object.keys(profile.environments);
  return profile.defaultMode ?? (availableModes.includes('testnet') ? 'testnet' :
    availableModes.includes('demo') ? 'demo' : undefined);
};

/**
 * One credential set per family and environment, in the environment a funded
 * smoke uses: the venue's test environment, mainnet only where it has none.
 * Names only; this never reads a value.
 */
export const liveSmokeCredentialSets = (names = Object.keys(profiles)) => {
  const sets = new Map();
  for (const name of names) {
    const profile = profiles[name];
    if (!profile) throw new Error(`Venue ${name} has no reviewed live-smoke endpoint profile`);
    const mode = testMode(profile) ?? 'mainnet';
    const entry = `${profile.family}_${mode.toUpperCase()}`;
    const set = sets.get(entry) ?? { entry, mode, venues: [], fields: Object.values(profile.fields),
      optionalFields: Object.values(profile.optionalFields ?? {}) };
    set.venues.push(name);
    sets.set(entry, set);
  }
  return [...sets.values()];
};

/**
 * Resolve environment before touching credentials. There is no mainnet fallback.
 * `credentials` (field suffix to value, e.g. from stdin) replaces the environment
 * entirely; `allowMissing` lets a dry run proceed without any credential.
 */
export const liveSmokeProfile = (name, requestedMode, env = process.env, { credentials, allowMissing = false } = {}) => {
  if (!Object.hasOwn(VENUES, name)) throw new Error('Unknown live-smoke venue');
  const profile = profiles[name];
  if (!profile) throw new Error('Venue has no reviewed live-smoke endpoint profile');
  const availableModes = Object.keys(profile.environments);
  const mode = requestedMode ?? testMode(profile);
  if (!mode) throw new Error('Venue has no engine-supported test environment; explicit --mainnet is required');
  if (!Object.hasOwn(profile.environments, mode)) throw new Error('Requested environment is unsupported by this venue profile');
  const options = { testnet: mode !== 'mainnet' && !(name === 'bybitdemo' && mode === 'demo'),
    publicDataOnly: false, ...profile.options };
  if (name === 'coinbasespot') options.spotOnly = true;
  const secrets = {};
  const credentialEnv = {};
  const suffixes = [...Object.values(profile.fields), ...Object.values(profile.optionalFields ?? {})];
  if (credentials !== undefined) {
    if (!credentials || typeof credentials !== 'object' || Array.isArray(credentials))
      throw new Error('Credentials must be one JSON object of field names');
    for (const field of Object.keys(credentials))
      // An inverted object would put a value in the name, so echo only name-shaped fields.
      if (!suffixes.includes(field))
        throw new Error(/^[A-Z0-9_]{1,40}$/.test(field) ? `Unknown credential field ${field}` : 'Unknown credential field');
  }
  const read = (path, suffix, required) => {
    const variable = `RUST_LIVE_SMOKE_${profile.family}_${mode.toUpperCase()}_${suffix}`;
    credentialEnv[path] = variable;
    // Errors name the field, never its value.
    const label = credentials === undefined ? `environment variable ${variable}` : `credential field ${suffix}`;
    const value = credentials === undefined ? env[variable] : credentials[suffix];
    if (value === undefined || value === '') {
      if (required && !allowMissing) throw new Error(`Missing ${label}`);
      return;
    }
    if (typeof value !== 'string' || !value.trim()) throw new Error(`Invalid ${label}`);
    if (['key', 'secret', 'passphrase', 'privateKey', 'omniKeySeed', 'apiKey', 'apiSecret', 'apiPassphrase'].includes(path.split('.').at(-1)) && value.length < 4)
      throw new Error(`Credential ${label} must contain at least four characters`);
    if (path === 'polymarket.marketSlugs') {
      const slugs = value.split(',').map((slug) => slug.trim());
      if (!slugs.length || slugs.length > 100 || slugs.some((slug) => !slug) || new Set(slugs).size !== slugs.length)
        throw new Error(`Invalid ${label}`);
      setOption(options, path, slugs);
    } else if (path === 'polymarket.signerType') {
      if (!/^[0-3]$/.test(value)) throw new Error(`Invalid ${label}`);
      setOption(options, path, Number(value));
    } else if (path === 'asterdex.apiVersion') {
      if (!['v1', 'v3'].includes(value)) throw new Error(`Invalid ${label}`);
      setOption(options, path, value);
    } else {
      secrets[path] = value;
      setOption(options, path, value);
    }
  };
  for (const [path, suffix] of Object.entries(profile.fields)) read(path, suffix, true);
  for (const [path, suffix] of Object.entries(profile.optionalFields ?? {})) read(path, suffix, false);
  if (profile.family === 'HYPERLIQUID' && options.key && options.applicationId &&
    options.key.toLowerCase() === options.applicationId.toLowerCase())
    throw new Error('Hyperliquid smoke requires an agent wallet distinct from the account');
  const endpointSet = profile.environments[mode];
  const allowUrl = (url, kind) => {
    try {
      const parsed = new URL(url);
      if (parsed.username || parsed.password || parsed.hash) return false;
      const protocolKind = parsed.protocol === 'https:' ? 'http' : parsed.protocol === 'wss:' ? 'ws' : undefined;
      return Boolean(protocolKind && (!kind || kind === protocolKind) && endpointSet[protocolKind].includes(parsed.origin));
    } catch { return false; }
  };
  const canonicalizeUrl = (url) => {
    if (!allowUrl(url)) throw new Error('URL is outside the selected venue environment');
    return new URL(url).href;
  };
  const simulationProduct = name === 'bitgetusdc' ? 'susdc-futures' :
    name === 'bitgetinverse' ? 'scoin-futures' : 'susdt-futures';
  const simulatedCoin = name === 'bitgetusdc' ? 'SUSDC' : 'SUSDT';
  const bitgetSimulation = profile.family === 'BITGET' && mode !== 'mainnet';
  const validateRequest = (request) => {
    if (!allowUrl(request.url, 'http')) throw new Error('HTTP URL is outside the selected venue environment');
    if (['OKX', 'OKX_EEA'].includes(profile.family) && mode === 'demo') {
      const headers = Array.isArray(request.headers) ? request.headers : Object.entries(request.headers ?? {});
      const markers = headers.filter(([key]) => key.toLowerCase() === 'x-simulated-trading');
      if (markers.length !== 1 || markers[0][1] !== '1')
        throw new Error('OKX demo request is missing its simulation marker');
    }
    if (!bitgetSimulation) return;
    const parsed = new URL(request.url);
    const method = (request.method ?? 'GET').toUpperCase();
    if (method === 'GET' && parsed.pathname === '/api/v3/account/settings' && !parsed.search) return;
    if (!parsed.pathname.startsWith('/api/v2/mix/') || !['GET', 'POST'].includes(method))
      throw new Error('Bitget simulation request is outside classic futures');
    let body = {};
    try { if (request.body) body = JSON.parse(request.body); }
    catch { throw new Error('Bitget simulation request body is invalid'); }
    const products = [...parsed.searchParams.getAll('productType')];
    const coins = [...parsed.searchParams.getAll('marginCoin')];
    const visit = (value) => {
      if (!value || typeof value !== 'object') return;
      for (const [key, item] of Object.entries(value)) {
        if (key === 'productType') products.push(item);
        if (key === 'marginCoin') coins.push(item);
        visit(item);
      }
    };
    visit(body);
    if (!products.length || products.some((product) => product !== simulationProduct) ||
      (method === 'POST' && body?.productType !== simulationProduct))
      throw new Error('Bitget request is missing its simulated product');
    if (coins.some((coin) => name === 'bitgetinverse' ?
      typeof coin !== 'string' || !/^[A-Z0-9]{1,20}$/.test(coin) || ['USDT', 'USDC'].includes(coin) :
      coin !== simulatedCoin))
      throw new Error('Bitget request has an unexpected simulation collateral');
  };
  const validateSocket = (url) => {
    if (!allowUrl(url, 'ws')) throw new Error('WebSocket URL is outside the selected venue environment');
    if (bitgetSimulation && !['/v2/ws/public', '/v2/ws/private'].includes(new URL(url).pathname))
      throw new Error('Bitget simulation does not support this socket');
  };
  const validateSocketMessage = (url, text) => {
    validateSocket(url);
    if (!bitgetSimulation || ['ping', 'pong'].includes(text)) return;
    let message;
    try { message = JSON.parse(text); }
    catch { throw new Error('Bitget simulation socket message is invalid'); }
    if (message.op === 'login' && new URL(url).pathname === '/v2/ws/private') return;
    if (!['subscribe', 'unsubscribe'].includes(message.op) || !Array.isArray(message.args) || !message.args.length ||
      message.args.some((arg) => arg?.instType !== simulationProduct.toUpperCase()))
      throw new Error('Bitget socket message is missing its simulated product');
  };
  return {
    venue: name, mode, availableModes, options, secrets,
    // Bitrue rejects non-GTC; Polymarket's engine does not send postOnly yet.
    postOnly: !['bitrue', 'polymarket'].includes(name),
    // A funded mainnet run only places, confirms, cancels and reads back.
    minimal: mode === 'mainnet',
    leverage: ['binancespot', 'bybitspot', 'coinbasespot', 'hyperliquidspot', 'krakenspot', 'polymarket'].includes(name) ? 'fixed' : 'supported',
    secretValues: Object.values(secrets), credentialEnv,
    endpoints: { http: [...endpointSet.http], ws: [...endpointSet.ws] },
    allowUrl, canonicalizeUrl, validateRequest, validateSocket, validateSocketMessage,
  };
};
