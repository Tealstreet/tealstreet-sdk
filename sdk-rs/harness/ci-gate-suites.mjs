// Exact CI coverage, independent of pass/fail policy. A known divergence can
// excuse a diagnostic, never a missing, duplicate or substituted replay case.
// Books are literal CLI arguments: private tapes deliberately use ''.
const primaryBooks = Object.freeze({
  breakoutprop: 'BTCUSD,ETHUSD',
  apexomni: 'BTCUSDT,ETHUSDT',
  asterdex: 'BTCUSDT,ETHUSDT',
  woo: 'BTCUSDT,ETHUSDT',
  binance: 'BTCUSDT,ETHUSDT,SOLUSDT',
  bitunix: 'BTCUSDT,ETHUSDT',
  orderly: 'BTCUSDC,ETHUSDC',
  coinbase: 'BTC-USD,ETH-USD',
  bitget: 'BTCUSDT,ETHUSDT',
  bitgetusdc: 'BTCUSDC,ETHUSDC',
  backpack: 'SOL_USDC_PERP,BTC_USDC_PERP',
  blofin: 'BTCUSDT,ETHUSDT',
  mexc: 'BTCUSDT,ETHUSDT',
  polymarket: 'xi-jinping-out-before-2027-Yes,will-gavin-newsom-win-the-2028-democratic-presidential-nomination-568-Yes',
  kucoin: 'BTCUSDT,ETHUSDT',
  kucoinspot: 'BTC-USDT,ETH-USDT',
  bingx: 'BTCUSDT,ETHUSDT',
  bybit: 'BTCUSDT,ETHUSDT',
  okx: 'BTCUSDT,ETHUSDT',
  cryptocom: 'BTCUSD-PERP,ETHUSD-PERP',
  weex: 'BTCUSDT,ETHUSDT',
  grvt: 'BTC_USDT_PERP,ETH_USDT_PERP',
  phemex: 'BTCUSDT,ETHUSDT',
  krakenspot: 'XBT/USD,ETH/USD',
  hyperliquidspot: 'PURR/USDC,HYPE/USDC',
  kraken: 'PF_XBTUSD,PF_ETHUSD',
  lighter: 'ETH,BTC',
  'lighter-robinhood': 'ETH,BTC',
  bitrue: 'E-BTC-USDT,E-ETH-USDT',
  hyperliquid: 'BTC-PERP,ETH-PERP',
  paradex: 'BTC-USD-PERP,ETH-USD-PERP',
  txflow: 'BTC-USDC,ETH-USDC',
});

export const GATE_SUITES = Object.freeze(Object.keys(primaryBooks));
const publicCase = (venue, books, fixture = venue) => ({ venue, tape: `sdk-rs/harness/fixtures/${fixture}-public.jsonl.gz`, books });
const privateCase = (venue, fixture = venue) => ({ venue, tape: `sdk-rs/harness/tapes/${fixture}-private.jsonl`, books: '' });
const xperpBooks = 'BTCUSD_UM_XPERP310404,ETHUSD_UM_XPERP310404';

const suiteCases = (suite) => {
  const cases = [publicCase(suite, primaryBooks[suite])];
  if (!['okx', 'orderly', 'coinbase'].includes(suite)) cases.push(privateCase(suite));
  if (suite === 'binance') {
    cases.push(publicCase('binancespot', 'BTCUSDT,ETHUSDT'),
      publicCase('binanceinverse', 'BTCUSD_PERP,ETHUSD_PERP'),
      publicCase('binancepm', 'BTCUSDT,ETHUSDT,SOLUSDT', 'binance'));
    cases.push(...['binancespot', 'binanceinverse', 'binancepm'].map((venue) => privateCase(venue)));
    cases.push({ venue: 'binance', tape: 'sdk-rs/harness/fixtures/binance-scripted-lifecycle-private.jsonl', books: 'BTCUSDT' });
  } else if (suite === 'bybit') {
    cases.push(publicCase('bybitinverse', 'BTCUSD,ETHUSD'),
      ...['bybitspot', 'bybitdemo', 'hyrotrader'].map((venue) => publicCase(venue, 'BTCUSDT,ETHUSDT')));
    cases.push(...['bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].map((venue) => privateCase(venue)));
  } else if (suite === 'okx') {
    cases.push(publicCase('okx', 'BTCUSD,ETHUSD', 'okx-inverse'),
      publicCase('okxeu', xperpBooks, 'okxeu-xperp'), publicCase('okxeu', xperpBooks));
    cases.push(...['okx', 'okxeu', 'okxdemo', 'okxeudemo'].map((venue) => privateCase(venue)));
  } else if (suite === 'orderly') {
    for (const venue of ['orderly', 'tealstreet-dex', 'kodiakfi', 'woofi', 'mode', 'salsadex', 'what']) {
      cases.push(privateCase(venue), privateCase(venue, `${venue}-testnet`));
    }
  } else if (suite === 'coinbase') {
    for (const profile of ['intx', 'cfm', 'spot'])
      for (const algorithm of ['ES256', 'EdDSA']) cases.push(privateCase('coinbase', `coinbase-${profile}-${algorithm}`));
    for (const algorithm of ['ES256', 'EdDSA']) cases.push(privateCase('coinbasespot', `coinbasespot-spot-${algorithm}`));
  } else if (suite === 'asterdex') {
    cases.push(privateCase('asterdex', 'asterdex-v3'));
  } else if (suite === 'lighter' || suite === 'lighter-robinhood') {
    cases.push(privateCase(suite, 'lighter-testnet'));
  } else if (suite === 'bitget') {
    cases.push(privateCase('bitgetinverse'));
  } else if (suite === 'hyperliquid') {
    cases.push(privateCase('hyperliquid', 'hyperliquid-spot'));
  }
  return Object.freeze(cases.map((item) => Object.freeze(item)));
};
const casesBySuite = Object.fromEntries(GATE_SUITES.map((suite) => [suite, suiteCases(suite)]));

export const expectedGateCases = (suite) => {
  if (!Object.hasOwn(casesBySuite, suite)) throw new Error(`Unknown replay gate suite ${JSON.stringify(suite)}`);
  return casesBySuite[suite];
};

export const gateCaseKey = (value) => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('case must be an object');
  for (const field of ['venue', 'tape', 'books']) {
    if (typeof value[field] !== 'string' || (field !== 'books' && !value[field].length))
      throw new Error(`${field} must be ${field === 'books' ? 'a string' : 'a nonempty string'}`);
  }
  return JSON.stringify([value.venue, value.tape, value.books]);
};

export const validateGateCoverage = (suite, cases) => {
  let expected;
  try { expected = new Map(expectedGateCases(suite).map((item) => [gateCaseKey(item), item])); }
  catch (error) { return [error.message]; }
  if (!Array.isArray(cases)) return ['gate cases must be an array'];
  const errors = [];
  const seen = new Map();
  for (const [index, item] of cases.entries()) {
    let key;
    try { key = gateCaseKey(item); }
    catch (error) { errors.push(`invalid gate case at index ${index}: ${error.message}`); continue; }
    if (seen.has(key)) errors.push(`duplicate gate case at indices ${seen.get(key)} and ${index}: ${key}`);
    else seen.set(key, index);
    if (!expected.has(key)) errors.push(`unexpected gate case: ${key}`);
  }
  for (const key of expected.keys()) if (!seen.has(key)) errors.push(`missing gate case: ${key}`);
  return errors;
};
