import { test } from 'node:test';
import assert from 'node:assert/strict';
import { liveSmokeCredentialSets, liveSmokeProfile, LIVE_SMOKE_VENUES } from './live-smoke-venues.mjs';

const env = new Proxy({}, { get: (_, key) => key.includes('_HYPERLIQUID_') && key.endsWith('SIGNER_ADDRESS') ? '0x' + '11'.repeat(20) :
  key.includes('_HYPERLIQUID_') && key.endsWith('ACCOUNT_ADDRESS') ? '0x' + '33'.repeat(20) :
  key.endsWith('SIGNER_TYPE') ? '0' :
  key.endsWith('API_VERSION') ? 'v3' : 'fake-credential' });

test('OKX smoke profiles use default TLS on the exact live/demo entity host', () => {
  for (const [name, mode, host] of [
    ['okx', 'mainnet', 'ws.okx.com'], ['okx', 'demo', 'wspap.okx.com'],
    ['okxeu', 'mainnet', 'wseea.okx.com'], ['okxeu', 'demo', 'wseeapap.okx.com'],
    ['okxdemo', 'demo', 'wspap.okx.com'], ['okxeudemo', 'demo', 'wseeapap.okx.com'],
  ]) {
    const profile = liveSmokeProfile(name, mode, env);
    for (const channel of ['public', 'private', 'business']) {
      assert.doesNotThrow(() => profile.validateSocket(`wss://${host}/ws/v5/${channel}`));
      assert.throws(() => profile.validateSocket(`wss://${host}:8443/ws/v5/${channel}`), /outside/);
      assert.throws(() => profile.validateSocket(`wss://${host}.evil/ws/v5/${channel}`), /outside/);
    }
  }
});

test('every registry name has an explicitly reviewed environment or fails closed', () => {
  for (const venue of LIVE_SMOKE_VENUES) {
    // KuCoin Spot needs a separate host and sizing review.
    if (['hyrotrader', 'grvt', 'breakoutprop', 'binancepm', 'kucoinspot', 'txflow', 'lighter-robinhood'].includes(venue)) {
      assert.throws(() => liveSmokeProfile(venue, 'mainnet', env), /no reviewed/);
      continue;
    }
    let profile;
    try { profile = liveSmokeProfile(venue, undefined, env); }
    catch (error) {
      assert.match(error.message, /explicit --mainnet/);
      profile = liveSmokeProfile(venue, 'mainnet', env);
    }
    for (const origin of [...profile.endpoints.http, ...profile.endpoints.ws])
      assert.equal(profile.allowUrl(`${origin}/path?x=1`), true, `${venue} ${origin}`);
    assert.equal(profile.allowUrl('https://example.com'), false);
  }
});

test('unreviewed profiles refuse all modes before credential access', () => {
  const env = new Proxy({}, { get: () => { throw new Error('credentials accessed'); } });
  for (const venue of ['hyrotrader', 'grvt', 'breakoutprop', 'binancepm', 'kucoinspot', 'txflow', 'lighter-robinhood'])
    for (const mode of [undefined, 'testnet', 'demo', 'mainnet'])
      assert.throws(() => liveSmokeProfile(venue, mode, env), /no reviewed/);
});

test('production-only modes refuse before credentials can be read', () => {
  const poisonedEnv = new Proxy({}, { get: () => { throw new Error('credentials accessed'); } });
  for (const venue of ['kucoin', 'coinbase', 'coinbasespot', 'backpack', 'mexc', 'bingx', 'weex', 'bitrue', 'bitunix', 'polymarket', 'krakenspot']) {
    assert.throws(() => liveSmokeProfile(venue, undefined, poisonedEnv), /explicit --mainnet/);
    assert.throws(() => liveSmokeProfile(venue, 'testnet', poisonedEnv), /unsupported/);
    assert.throws(() => liveSmokeProfile(venue, 'demo', poisonedEnv), /unsupported/);
  }
});

test('default selects simulation and excludes production hosts', () => {
  const binance = liveSmokeProfile('binance', undefined, env);
  assert.equal(binance.mode, 'testnet');
  assert.equal(binance.options.testnet, true);
  assert.equal(binance.allowUrl('https://fapi.binance.com/fapi/v1/order'), false);
  assert.equal(binance.allowUrl('https://demo-fapi.binance.com/fapi/v1/order'), true);
  assert.equal(liveSmokeProfile('bybitdemo', undefined, env).mode, 'demo');
  assert.equal(liveSmokeProfile('bybitdemo', undefined, env).options.testnet, false);
  assert.equal(liveSmokeProfile('bybitdemo', 'testnet', env).options.testnet, true);
  assert.equal(liveSmokeProfile('okx', undefined, env).mode, 'demo');
  assert.equal(liveSmokeProfile('kraken', undefined, env).mode, 'demo');
});

test('OKX demo rejects requests without exactly one simulated-trading marker', () => {
  const profile = liveSmokeProfile('okxeu', 'demo', env);
  const request = { url: 'https://eea.okx.com/api/v5/trade/order', method: 'POST' };
  for (const headers of [{}, { 'x-simulated-trading': '0' },
    [['x-simulated-trading', '1'], ['X-Simulated-Trading', '0']]])
    assert.throws(() => profile.validateRequest({ ...request, headers }), /simulation marker/);
  profile.validateRequest({ ...request, headers: { 'X-Simulated-Trading': '1' } });
});

test('Bitget shared-origin requests bind the simulated product and collateral', () => {
  const profile = liveSmokeProfile('bitget', 'testnet', env);
  const request = { url: 'https://api.bitget.com/api/v2/mix/order/place-order', method: 'POST' };
  for (const body of [{}, { productType: 'usdt-futures', marginCoin: 'USDT' },
    { productType: 'susdt-futures', marginCoin: 'USDT' },
    { productType: 'susdt-futures', orders: [{ productType: 'usdt-futures' }] }])
    assert.throws(() => profile.validateRequest({ ...request, body: JSON.stringify(body) }), /simulat/);
  profile.validateRequest({ ...request, body: JSON.stringify({ productType: 'susdt-futures', marginCoin: 'SUSDT' }) });
  profile.validateRequest({ method: 'GET', url: 'https://api.bitget.com/api/v2/mix/account/accounts?productType=susdt-futures' });
  profile.validateRequest({ method: 'GET', url: 'https://api.bitget.com/api/v3/account/settings' });
  assert.throws(() => profile.validateRequest({ method: 'POST', url: 'https://api.bitget.com/api/v3/trade/place-order',
    body: '{"productType":"susdt-futures"}' }), /outside classic/);
  assert.throws(() => profile.validateRequest({ method: 'POST', url: 'https://api.bitget.com/api/v3/account/settings' }), /outside classic/);
});

test('Bitget socket guards reject production subscriptions and unsupported UTA sockets', () => {
  const profile = liveSmokeProfile('bitgetusdc', 'testnet', env);
  const url = 'wss://ws.bitget.com/v2/ws/private';
  profile.validateSocketMessage(url, '{"op":"login","args":[]}');
  profile.validateSocketMessage(url, '{"op":"subscribe","args":[{"instType":"SUSDC-FUTURES"}]}');
  for (const text of ['{"op":"subscribe","args":[{"instType":"USDC-FUTURES"}]}',
    '{"op":"subscribe","args":[]}', '{"op":"place-order"}', '{}'])
    assert.throws(() => profile.validateSocketMessage(url, text), /simulated product/);
  assert.throws(() => profile.validateSocket('wss://ws.bitget.com/v3/ws/private'), /does not support/);
});

test('credential errors reveal names only', () => {
  assert.throws(() => liveSmokeProfile('binance', 'testnet', {}), /^Error: Missing environment variable RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY$/);
  assert.throws(() => liveSmokeProfile('polymarket', 'mainnet', {
    RUST_LIVE_SMOKE_POLYMARKET_MAINNET_PRIVATE_KEY: 'private-key-must-not-appear',
    RUST_LIVE_SMOKE_POLYMARKET_MAINNET_SIGNER_TYPE: 'bad-private-config-must-not-appear',
  }), /^Error: Invalid environment variable RUST_LIVE_SMOKE_POLYMARKET_MAINNET_SIGNER_TYPE$/);
});

test('short credential material refuses before I/O while numeric public indices remain valid', () => {
  assert.throws(() => liveSmokeProfile('binance', 'testnet', {
    RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY: 'fake-key', RUST_LIVE_SMOKE_BINANCE_TESTNET_SECRET: 'abc',
  }), (error) => error.message.includes('at least four') && !error.message.includes('abc'));
  const lighter = liveSmokeProfile('lighter', 'testnet', {
    RUST_LIVE_SMOKE_LIGHTER_TESTNET_PRIVATE_KEY: 'fake-signing-key', RUST_LIVE_SMOKE_LIGHTER_TESTNET_ACCOUNT_INDEX: '1', RUST_LIVE_SMOKE_LIGHTER_TESTNET_API_KEY_INDEX: '2',
  });
  assert.equal(lighter.options.applicationId, '1');
  assert.equal(lighter.options.apiKeyIndex, '2');
});

test('profiles accept no URL credentials, fragments, insecure schemes, host suffixes or alternate ports', () => {
  const profile = liveSmokeProfile('binance', 'testnet', env);
  for (const url of [
    'http://demo-fapi.binance.com/x', 'https://demo-fapi.binance.com.evil.example/x',
    'https://demo-fapi.binance.com:123/x', 'https://u:p@demo-fapi.binance.com/x',
    'https://demo-fapi.binance.com/x#fragment', 'wss://fstream.binance.com/ws',
  ]) {
    assert.equal(profile.allowUrl(url), false, url);
    assert.throws(() => profile.canonicalizeUrl(url), /outside/);
  }
  assert.equal(profile.allowUrl('https://demo-fapi.binance.com/x', 'ws'), false);
  assert.equal(profile.canonicalizeUrl('https://DEMO-FAPI.BINANCE.COM:443/x?x=1'), 'https://demo-fapi.binance.com/x?x=1');
});

test('aliases share env names and nested configuration is typed', () => {
  assert.equal(liveSmokeProfile('bybitinverse', 'testnet', env).credentialEnv.key, 'RUST_LIVE_SMOKE_BYBIT_TESTNET_KEY');
  const polymarket = liveSmokeProfile('polymarket', 'mainnet', env);
  assert.equal(polymarket.options.polymarket.signerType, 0);
  assert.equal(polymarket.secrets['polymarket.signerType'], undefined);
  assert.equal(polymarket.options.polymarket.apiKey, 'fake-credential');
  assert.equal(liveSmokeProfile('asterdex', 'testnet', env).options.asterdex.apiVersion, 'v3');
});

test('Orderly allows its distinct public and private hosts', () => {
  const profile = liveSmokeProfile('orderly', 'testnet', env);
  assert.equal(profile.allowUrl('wss://testnet-ws-private-evm.orderly.org/v2/ws/private/stream/id'), true);
  assert.equal(profile.allowUrl('wss://ws-private-evm.orderly.org/v2/ws/private/stream/id'), false);
});

test('fresh profile mutation cannot change another profile allowlist or options', () => {
  const first = liveSmokeProfile('binance', 'testnet', env);
  first.endpoints.http.push('https://example.com');
  first.options.testnet = false;
  const second = liveSmokeProfile('binance', 'testnet', env);
  assert.equal(first.allowUrl('https://example.com'), false);
  assert.equal(second.options.testnet, true);
  assert.equal(second.endpoints.http.includes('https://example.com'), false);
  assert.throws(() => liveSmokeProfile('__proto__', 'mainnet', env), /Unknown/);
});

test('Polymarket discovery can be scoped to a bounded public market selection', () => {
  const env = { RUST_LIVE_SMOKE_POLYMARKET_MAINNET_PRIVATE_KEY: 'fake-private-key',
    RUST_LIVE_SMOKE_POLYMARKET_MAINNET_MARKET_SLUGS: 'first-market,second-market' };
  const profile = liveSmokeProfile('polymarket', 'mainnet', env);
  assert.deepEqual(profile.options.polymarket.marketSlugs, ['first-market', 'second-market']);
  assert.equal(profile.secrets['polymarket.marketSlugs'], undefined);
  assert.throws(() => liveSmokeProfile('polymarket', 'mainnet', { ...env, RUST_LIVE_SMOKE_POLYMARKET_MAINNET_MARKET_SLUGS: 'first-market,first-market' }), /Invalid environment variable/);
});


test('each mode reads its own credentials and never falls back to legacy or mainnet keys', () => {
  const mainnet = { RUST_LIVE_SMOKE_BINANCE_MAINNET_KEY: 'mainnet-key', RUST_LIVE_SMOKE_BINANCE_MAINNET_SECRET: 'mainnet-secret',
    RUST_LIVE_SMOKE_BINANCE_KEY: 'legacy-mainnet-key', RUST_LIVE_SMOKE_BINANCE_SECRET: 'legacy-mainnet-secret' };
  assert.throws(() => liveSmokeProfile('binance', 'testnet', mainnet), /BINANCE_TESTNET_KEY/);
  const supplied = { ...mainnet, RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY: 'testnet-key', RUST_LIVE_SMOKE_BINANCE_TESTNET_SECRET: 'testnet-secret' };
  assert.equal(liveSmokeProfile('binance', 'testnet', supplied).options.key, 'testnet-key');
  assert.equal(liveSmokeProfile('binance', 'mainnet', supplied).options.key, 'mainnet-key');
  const accessed = [];
  for (const mode of ['testnet', 'demo']) {
    const profile = liveSmokeProfile('bybitdemo', mode, new Proxy({}, { get(_target, key) { accessed.push(key); return 'fake-secret'; } }));
    assert.equal(profile.credentialEnv.key, `RUST_LIVE_SMOKE_BYBIT_${mode.toUpperCase()}_KEY`);
  }
  assert.deepEqual(accessed, ['RUST_LIVE_SMOKE_BYBIT_TESTNET_KEY', 'RUST_LIVE_SMOKE_BYBIT_TESTNET_SECRET',
    'RUST_LIVE_SMOKE_BYBIT_DEMO_KEY', 'RUST_LIVE_SMOKE_BYBIT_DEMO_SECRET']);
});

test('reviewed venue profiles enable post-only whenever the engine supports it', () => {
  for (const venue of LIVE_SMOKE_VENUES) {
    let profile;
    try { profile = liveSmokeProfile(venue, undefined, env); }
    catch { try { profile = liveSmokeProfile(venue, 'mainnet', env); } catch { continue; } }
    assert.equal(profile.postOnly, !['bitrue', 'polymarket'].includes(venue), venue);
  }
});

test('Binance Spot defaults to its testnet and allows exactly its own hosts', () => {
  const spot = liveSmokeProfile('binancespot', undefined, env);
  assert.equal(spot.mode, 'testnet');
  assert.equal(spot.options.testnet, true);
  assert.equal(spot.leverage, 'fixed');
  assert.equal(spot.allowUrl('https://testnet.binance.vision/api/v3/order'), true);
  assert.equal(spot.allowUrl('wss://ws-api.testnet.binance.vision/ws-api/v3', 'ws'), true);
  assert.equal(spot.allowUrl('https://api.binance.com/api/v3/order'), false);
  assert.equal(spot.allowUrl('https://demo-fapi.binance.com/fapi/v1/order'), false);
  const mainnet = liveSmokeProfile('binancespot', 'mainnet', env);
  assert.equal(mainnet.options.testnet, false);
  assert.equal(mainnet.allowUrl('wss://ws-api.binance.com:443/ws-api/v3', 'ws'), true);
  assert.equal(mainnet.allowUrl('wss://stream.binance.com:9443/ws', 'ws'), true);
  assert.equal(mainnet.allowUrl('wss://stream.binance.com/ws', 'ws'), false);
  assert.equal(mainnet.allowUrl('https://fapi.binance.com/fapi/v1/order'), false);
  assert.throws(() => liveSmokeProfile('binancespot', 'mainnet', {}),
    /^Error: Missing environment variable RUST_LIVE_SMOKE_BINANCESPOT_MAINNET_KEY$/);
});

test('Hyperliquid Spot uses the same agent and account fields with fixed leverage', () => {
  const prefix = 'RUST_LIVE_SMOKE_HYPERLIQUID_MAINNET_';
  const values = { [prefix + 'SIGNER_ADDRESS']: '0x' + '11'.repeat(20),
    [prefix + 'PRIVATE_KEY']: '0x' + '22'.repeat(32),
    [prefix + 'ACCOUNT_ADDRESS']: '0x' + '33'.repeat(20) };
  const perps = liveSmokeProfile('hyperliquid', 'mainnet', values);
  const spot = liveSmokeProfile('hyperliquidspot', 'mainnet', values);
  assert.deepEqual(spot.credentialEnv, perps.credentialEnv);
  assert.deepEqual(spot.options, perps.options);
  assert.notEqual(spot.options.key, spot.options.applicationId);
  assert.equal(spot.leverage, 'fixed');
  assert.equal(spot.postOnly, true);
  assert.equal(spot.options.publicDataOnly, false);
  spot.validateRequest({ method: 'POST', url: 'https://api.hyperliquid.xyz/exchange' });
  spot.validateSocket('wss://api-ui.hyperliquid.xyz/ws');
  for (const url of ['https://api.hyperliquid-testnet.xyz/info',
    'https://api.hyperliquid.xyz.evil.example/exchange', 'https://api.hyperliquid.xyz:123/exchange'])
    assert.throws(() => spot.validateRequest({ method: 'POST', url }), /outside/);
  assert.throws(() => spot.validateSocket('wss://api.hyperliquid-testnet.xyz/ws'), /outside/);
  const poisoned = new Proxy({}, { get: () => { throw Error('credentials accessed'); } });
  assert.throws(() => liveSmokeProfile('hyperliquidspot', 'demo', poisoned), /unsupported/);
  assert.throws(() => liveSmokeProfile('hyperliquidspot', 'mainnet', {}), /SIGNER_ADDRESS/);
  const testnet = liveSmokeProfile('hyperliquidspot', undefined, env);
  assert.equal(testnet.mode, 'testnet');
  assert.equal(testnet.allowUrl('https://api.hyperliquid.xyz/info'), false);
});


test('Hyperliquid smoke refuses an account wallet supplied as the agent', () => {
  const prefix = 'RUST_LIVE_SMOKE_HYPERLIQUID_MAINNET_';
  const values = { [prefix + 'SIGNER_ADDRESS']: '0x' + 'aa'.repeat(20),
    [prefix + 'PRIVATE_KEY']: 'fixture-agent-secret', [prefix + 'ACCOUNT_ADDRESS']: '0x' + 'AA'.repeat(20) };
  for (const venue of ['hyperliquid', 'hyperliquidspot'])
    assert.throws(() => liveSmokeProfile(venue, 'mainnet', values), /^Error: Hyperliquid smoke requires an agent wallet distinct from the account$/);
});

test('Kraken Spot requires explicit mainnet before touching its separate credentials', () => {
  const poisoned = new Proxy({}, { get() { throw new Error('credentials accessed'); } });
  assert.throws(() => liveSmokeProfile('krakenspot', undefined, poisoned), /explicit --mainnet/);
  for (const mode of ['testnet', 'demo'])
    assert.throws(() => liveSmokeProfile('krakenspot', mode, poisoned), /unsupported/);
  const profile = liveSmokeProfile('krakenspot', 'mainnet', env);
  assert.equal(profile.credentialEnv.key, 'RUST_LIVE_SMOKE_KRAKEN_SPOT_MAINNET_KEY');
  assert.equal(profile.credentialEnv.secret, 'RUST_LIVE_SMOKE_KRAKEN_SPOT_MAINNET_SECRET');
  assert.equal(profile.leverage, 'fixed');
  assert.equal(profile.postOnly, true);
  assert.equal(profile.options.testnet, false);
  assert.equal(profile.options.publicDataOnly, false);
});

test('Kraken Spot endpoint fences exclude Futures, beta, impostor hosts and alternate ports', () => {
  const profile = liveSmokeProfile('krakenspot', 'mainnet', env);
  profile.validateRequest({ method: 'POST', url: 'https://api.kraken.com/0/private/AddOrder' });
  profile.validateSocket('wss://ws.kraken.com/v2');
  profile.validateSocket('wss://ws-auth.kraken.com/v2');
  for (const url of ['https://futures.kraken.com/derivatives/api/v3/sendorder',
    'https://demo-futures.kraken.com/derivatives/api/v3/sendorder',
    new URL('/0/private/AddOrder', ['https://api.kraken.com', 'evil.example'].join('.')).href,
    new URL('/0/private/AddOrder', 'https://api.kraken.com:8443').href])
    assert.throws(() => profile.validateRequest({ method: 'POST', url }), /outside/);
  for (const url of ['wss://beta-ws.kraken.com/v2', 'wss://beta-ws-auth.kraken.com/v2',
    'wss://futures.kraken.com/ws/v1', 'wss://ws-auth.kraken.com:8443/v2'])
    assert.throws(() => profile.validateSocket(url), /outside/);
});

test('Kraken Spot never borrows Futures keys and the Futures default stays demo', () => {
  const futuresOnly = { RUST_LIVE_SMOKE_KRAKEN_MAINNET_KEY: 'fixture-key',
    RUST_LIVE_SMOKE_KRAKEN_MAINNET_SECRET: 'fixture-secret' };
  assert.throws(() => liveSmokeProfile('krakenspot', 'mainnet', futuresOnly), /KRAKEN_SPOT_MAINNET_KEY/);
  assert.equal(liveSmokeProfile('kraken', undefined, env).mode, 'demo');
});

test('stdin credentials replace the environment and refuse unknown or missing fields by name only', () => {
  const environment = { RUST_LIVE_SMOKE_BITUNIX_MAINNET_KEY: 'env-key-not-used', RUST_LIVE_SMOKE_BITUNIX_MAINNET_SECRET: 'env-secret-not-used' };
  const profile = liveSmokeProfile('bitunix', 'mainnet', environment, { credentials: { KEY: 'stdin-key', SECRET: 'stdin-secret' } });
  assert.equal(profile.options.key, 'stdin-key');
  assert.equal(profile.options.secret, 'stdin-secret');
  assert.deepEqual(profile.secretValues.sort(), ['stdin-key', 'stdin-secret']);
  assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', environment, { credentials: { KEY: 'stdin-key' } }),
    /^Error: Missing credential field SECRET$/);
  assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', {}, { credentials: { KEY: 'stdin-key', SECRET: 'stdin-secret', PASSWORD: 'value-must-not-appear' } }),
    (error) => error.message === 'Unknown credential field PASSWORD');
  assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', {}, { credentials: { 'inverted-secret-value': 'KEY', SECRET: 'stdin-secret' } }),
    (error) => error.message === 'Unknown credential field');
  assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', {}, { credentials: { KEY: 'stdin-key', SECRET: 'abc' } }),
    (error) => error.message.includes('at least four') && !error.message.includes('abc'));
  for (const credentials of [null, [], 'text']) assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', {}, { credentials }), /one JSON object/);
});

test('a dry run may resolve a profile without credentials', () => {
  const profile = liveSmokeProfile('bitunix', 'mainnet', {}, { allowMissing: true });
  assert.deepEqual(profile.secrets, {});
  assert.equal(profile.minimal, true);
  assert.equal(liveSmokeProfile('hyperliquid', 'testnet', {}, { allowMissing: true }).mode, 'testnet');
  assert.throws(() => liveSmokeProfile('bitunix', 'mainnet', {}), /Missing environment variable RUST_LIVE_SMOKE_BITUNIX_MAINNET_KEY/);
});

test('only mainnet runs use the minimal sequence', () => {
  assert.equal(liveSmokeProfile('binance', 'testnet', env).minimal, false);
  assert.equal(liveSmokeProfile('okx', 'demo', env).minimal, false);
  assert.equal(liveSmokeProfile('krakenspot', 'mainnet', env).minimal, true);
});

test('OKX EEA keys are a separate credential family from global OKX', () => {
  assert.equal(liveSmokeProfile('okx', 'demo', env).credentialEnv.key, 'RUST_LIVE_SMOKE_OKX_DEMO_KEY');
  assert.equal(liveSmokeProfile('okxdemo', 'demo', env).credentialEnv.key, 'RUST_LIVE_SMOKE_OKX_DEMO_KEY');
  assert.equal(liveSmokeProfile('okxeu', 'demo', env).credentialEnv.key, 'RUST_LIVE_SMOKE_OKX_EEA_DEMO_KEY');
  assert.equal(liveSmokeProfile('okxeudemo', 'demo', env).credentialEnv.passphrase, 'RUST_LIVE_SMOKE_OKX_EEA_DEMO_PASSPHRASE');
});

test('credential sets use the test environment where one exists and mainnet only otherwise', () => {
  const sets = Object.fromEntries(liveSmokeCredentialSets().map((set) => [set.entry, set]));
  const entryOf = (venue) => Object.values(sets).find((set) => set.venues.includes(venue))?.entry;
  assert.deepEqual(Object.fromEntries(['binance', 'binancespot', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'okx', 'okxdemo',
    'okxeu', 'okxeudemo', 'bitget', 'bitgetusdc', 'kraken', 'krakenspot', 'hyperliquid', 'hyperliquidspot', 'lighter', 'paradex',
    'bitunix', 'phemex', 'bingx', 'apexomni', 'blofin', 'cryptocom', 'asterdex'].map((venue) => [venue, entryOf(venue)])), {
    binance: 'BINANCE_TESTNET', binancespot: 'BINANCESPOT_TESTNET', bybit: 'BYBIT_TESTNET', bybitinverse: 'BYBIT_TESTNET',
    bybitspot: 'BYBIT_TESTNET', bybitdemo: 'BYBIT_DEMO', okx: 'OKX_DEMO', okxdemo: 'OKX_DEMO', okxeu: 'OKX_EEA_DEMO',
    okxeudemo: 'OKX_EEA_DEMO', bitget: 'BITGET_TESTNET', bitgetusdc: 'BITGET_TESTNET', kraken: 'KRAKEN_DEMO',
    krakenspot: 'KRAKEN_SPOT_MAINNET', hyperliquid: 'HYPERLIQUID_TESTNET', hyperliquidspot: 'HYPERLIQUID_TESTNET',
    lighter: 'LIGHTER_TESTNET', paradex: 'PARADEX_TESTNET', bitunix: 'BITUNIX_MAINNET', phemex: 'PHEMEX_TESTNET',
    bingx: 'BINGX_MAINNET', apexomni: 'APEXOMNI_TESTNET', blofin: 'BLOFIN_DEMO', cryptocom: 'CRYPTOCOM_TESTNET',
    asterdex: 'ASTERDEX_TESTNET',
  });
  assert.deepEqual(sets.HYPERLIQUID_TESTNET.fields, ['SIGNER_ADDRESS', 'PRIVATE_KEY', 'ACCOUNT_ADDRESS']);
  assert.deepEqual(sets.OKX_EEA_DEMO.fields, ['KEY', 'SECRET', 'PASSPHRASE']);
  assert.ok(sets.ASTERDEX_TESTNET.optionalFields.includes('API_VERSION'));
  assert.throws(() => liveSmokeCredentialSets(['binancepm']), /no reviewed live-smoke endpoint profile/);
  assert.deepEqual(liveSmokeCredentialSets(['binance', 'binanceinverse']).map((set) => [set.entry, set.venues]),
    [['BINANCE_TESTNET', ['binance', 'binanceinverse']]]);
});

test('Binance COIN-M uses the futures demo keys on dapi hosts only', () => {
  const inverse = liveSmokeProfile('binanceinverse', undefined, env);
  assert.equal(inverse.mode, 'testnet');
  assert.equal(inverse.credentialEnv.key, 'RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY');
  assert.equal(inverse.allowUrl('https://demo-dapi.binance.com/dapi/v1/order'), true);
  assert.equal(inverse.allowUrl('wss://dstream.binancefuture.com/ws/listen-key', 'ws'), true);
  for (const url of ['https://dapi.binance.com/dapi/v1/order', 'https://demo-fapi.binance.com/fapi/v1/order'])
    assert.equal(inverse.allowUrl(url), false, url);
  const mainnet = liveSmokeProfile('binanceinverse', 'mainnet', env);
  assert.equal(mainnet.allowUrl('https://dapi.binance.com/dapi/v1/order'), true);
  assert.equal(mainnet.allowUrl('wss://dstream.binance.com/ws', 'ws'), true);
  assert.equal(mainnet.allowUrl('https://demo-dapi.binance.com/dapi/v1/order'), false);
});
