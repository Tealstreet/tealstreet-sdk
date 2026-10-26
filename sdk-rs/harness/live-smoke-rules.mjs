// Only public discovery responses establish sizing. A missing monetary field is
// unknown unless the cited venue contract defines a quantity-only minimum.
export const LIVE_SMOKE_RULE_SOURCES = Object.freeze({
  binance: 'https://developers.binance.com/docs/derivatives/usds-margined-futures/common-definition',
  binancespot: 'https://developers.binance.com/docs/binance-spot-api-docs/filters',
  asterdex: 'https://github.com/asterdex/api-docs/blob/master/V3%28Recommended%29/EN/aster-finance-futures-api-v3.md',
  bybit: 'https://bybit-exchange.github.io/docs/v5/market/instrument',
  bitget: 'https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market',
  bitrue: 'https://www.bitrue.com/api_docs_includes_file/futures/index.html',
  bingx: 'https://github.com/BingX-API/api-ai-skills/blob/main/skills/swap-market/api-reference.md',
  orderly: 'https://orderly.network/docs/build-on-omnichain/restful-api/public/get-available-symbols',
  woo: 'https://docs.woox.io/#available-symbols-public',
  lighter: 'https://github.com/elliottech/lighter-python/blob/main/lighter/models/order_book_details.py',
  paradex: 'https://docs.paradex.trade/api/testnet/markets/get-markets',
  hyperliquid: 'https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/error-responses',
  okx: 'https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments',
  blofin: 'https://docs.blofin.com/index.html#get-instruments',
  apexomni: 'https://api-docs.pro.apex.exchange/#get-configs',
  backpack: 'https://docs.backpack.exchange/#tag/Markets/operation/get_markets',
  kraken: 'https://support.kraken.com/articles/4844359082772-linear-multi-collateral-derivatives-contract-specifications',
  krakenspot: 'https://docs.kraken.com/api-reference/market-data/get-tradable-asset-pairs',
  kucoin: 'https://www.kucoin.com/docs-new/rest/futures-trading/market-data/get-all-symbols',
  mexc: 'https://mexcdevelop.github.io/apidocs/contract_v1_en/#get-contract-details',
  weex: 'https://www.weex.com/legacy-docs/contract/Market_API/GetContractInfo',
  polymarket: 'https://docs.polymarket.com/trading/place-orders',
  coinbase: 'https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/products/list-products',
});

// These engine discovery shapes do not establish the complete monetary floor.
// Do not turn their absence into an inferred zero.
export const LIVE_SMOKE_UNKNOWN_RULES = Object.freeze({
  phemex: 'PerpetualV2 qtyStepSize alone does not establish a minimum order value.',
  cryptocom: 'qty_tick_size alone does not establish a minimum order value.',
  hyrotrader: 'No reviewed endpoint profile.',
  grvt: 'No reviewed endpoint profile or sizing contract.',
  kucoinspot: 'No reviewed Spot endpoint profile or sizing contract.',
});

const unknown = () => ({ notionalKnown: false, minNotional: 0 });
const decimal = (value) => {
  if (typeof value !== 'number' && (typeof value !== 'string' ||
    !/^[+]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(value))) return undefined;
  const number = Number(value);
  return Number.isFinite(number) && number >= 0 ? number : undefined;
};
const positive = (value) => { const number = decimal(value); return number > 0 ? number : undefined; };
const decimalStep = (value, signed = false) => Number.isInteger(value) && value >= (signed ? -18 : 0) && value <= 18 ?
  Number(`1e${-value}`) : undefined;
const smallerMaximum = (...values) => {
  const supplied = values.filter((value) => value !== undefined);
  if (!supplied.length) return undefined;
  return supplied.every((value) => positive(value)) ? Math.min(...supplied.map(Number)) : NaN;
};
const array = (value) => Array.isArray(value) ? value : [];
const code = (value, expected = '0') => String(value) === expected;
const matches = (row, market, ...keys) => keys.some((key) => typeof row?.[key] === 'string' &&
  row[key] !== '' && [market.id, market.symbol].includes(row[key]));
const familyOf = (name) => {
  if (['orderly', 'tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'].includes(name)) return 'orderly';
  if (['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo'].includes(name)) return 'bybit';
  if (['bitget', 'bitgetusdc', 'bitgetinverse'].includes(name)) return 'bitget';
  if (['okx', 'okxeu', 'okxdemo', 'okxeudemo'].includes(name)) return 'okx';
  if (name === 'coinbasespot') return 'coinbase';
  return name;
};
const hosts = {
  binance: /^(?:demo-)?fapi\.binance\.com$/,
  binancespot: /^(?:api\.binance\.com|testnet\.binance\.vision)$/,
  asterdex: /^fapi\.asterdex(?:-testnet)?\.com$/,
  bybit: /^api(?:-testnet|-demo)?\.bybit\.com$/,
  bitget: /^api\.bitget\.com$/,
  bitrue: /^fapi\.bitrue\.com$/,
  bingx: /^open-api\.bingx\.com$/,
  orderly: /^(?:testnet-)?api-evm\.orderly\.org$/,
  woo: /^api\.(?:staging\.)?woox\.io$/,
  lighter: /^(?:testnet-api\.lighter\.xyz|mainnet\.zklighter\.elliot\.ai)$/,
  paradex: /^api\.(?:prod|testnet)\.paradex\.trade$/,
  hyperliquid: /^api\.hyperliquid(?:-testnet)?\.xyz$/,
  okx: /^(?:www|openapi|eea)\.okx\.com$/,
  blofin: /^(?:openapi|demo-trading-openapi)\.blofin\.com$/,
  apexomni: /^(?:testnet\.)?omni\.apex\.exchange$/,
  backpack: /^api\.backpack\.exchange$/,
  kraken: /^(?:demo-)?futures\.kraken\.com$/,
  krakenspot: /^api\.kraken\.com$/,
  kucoin: /^api-futures\.kucoin\.com$/,
  mexc: /^api\.mexc\.com$/,
  weex: /^api-contract\.weex\.com$/,
  polymarket: /^(?:gamma-api|clob)\.polymarket\.com$/,
  coinbase: /^api\.(?:international\.)?coinbase\.com$/,
};

function known(notional, minimum, step, scale = 1, quoteQuantity = false, lowerMultiplier, maximum) {
  const minNotional = decimal(notional), minimumAmount = decimal(minimum), amountStep = positive(step);
  const notionalPerAmount = positive(scale);
  const maximumAmount = maximum === undefined ? undefined : positive(maximum);
  if (minNotional === undefined || minimumAmount === undefined || !amountStep || !notionalPerAmount ||
    (maximum !== undefined && (!maximumAmount || maximumAmount < Math.max(minimumAmount, amountStep))))
    return unknown();
  return { notionalKnown: true, minNotional, minimumAmount: Math.max(minimumAmount, amountStep),
    amountStep, notionalPerAmount, quoteQuantity,
    ...(lowerMultiplier === undefined ? {} : { lowerMultiplier }),
    ...(maximumAmount === undefined ? {} : { maximumAmount }) };
}

function filtered(row) {
  const filters = array(row.filters).filter((filter) => filter && typeof filter === 'object');
  const lot = filters.find((filter) => filter.filterType === 'LOT_SIZE');
  const notionals = filters.filter((filter) => ['MIN_NOTIONAL', 'NOTIONAL'].includes(filter.filterType));
  if (!lot || !notionals.length) return unknown();
  const values = notionals.map((filter) => decimal(filter.notional ?? filter.minNotional));
  if (values.some((value) => value === undefined)) return unknown();
  const bounds = filters.filter((filter) => filter.filterType === 'PERCENT_PRICE').map((filter) => positive(filter.multiplierDown));
  if (bounds.some((value) => value === undefined)) return unknown();
  return known(Math.max(...values), lot.minQty, lot.stepSize, 1, false, bounds.length ? Math.max(...bounds) : undefined, lot.maxQty);
}

// The optional returned amount fields are in PLACE/AMEND units. Store order
// quantities may use different units (notably OKX, BloFin, KuCoin and MEXC).
export function venueMarketRules(venue, records, market) {
  const name = typeof venue === 'string' ? venue : venue?.rust;
  const family = familyOf(name);
  if (!hosts[family] || !market || typeof market.symbol !== 'string') return unknown();
  let result = unknown();
  for (const record of records) {
    if (record.type !== 'http') continue;
    let url, payload;
    try {
      url = new URL(record.url);
      if (url.protocol !== 'https:' || url.port || url.username || url.password || !hosts[family].test(url.hostname)) continue;
      // AssetPairs is a complete catalog: a later failed or malformed reply
      // cannot authorize sizing from a previously selected pair.
      if (family === 'krakenspot' && record.method?.toUpperCase() === 'GET' && url.pathname === '/0/public/AssetPairs') result = unknown();
      if (!(record.status >= 200 && record.status < 300)) continue;
      payload = JSON.parse(record.body);
      if (!payload || typeof payload !== 'object') continue;
    } catch { continue; }
    const path = url.pathname;
    const method = record.method?.toUpperCase();
    let rows = [], match = (row) => matches(row, market, 'symbol'), rule;
    switch (family) {
      case 'binance': case 'asterdex':
        if (method !== 'GET' || !(family === 'asterdex' ? ['/fapi/v1/exchangeInfo', '/fapi/v3/exchangeInfo'] :
          ['/fapi/v1/exchangeInfo']).includes(path)) continue;
        rows = array(payload.symbols); rule = filtered; break;
      case 'binancespot':
        // Spot LOT_SIZE/NOTIONAL as futures; PERCENT_PRICE_BY_SIDE claims no lower bound here.
        if (method !== 'GET' || path !== '/api/v3/exchangeInfo') continue;
        rows = array(payload.symbols); rule = filtered; break;
      case 'bybit': {
        if (method !== 'GET' || path !== '/v5/market/instruments-info' || !code(payload.retCode)) continue;
        const category = name === 'bybitspot' ? 'spot' : name === 'bybitinverse' ? 'inverse' : 'linear';
        if (url.searchParams.get('category') !== category || payload.result?.category !== category) continue;
        rows = array(payload.result.list);
        rule = (row) => {
          const lot = row.lotSizeFilter;
          if (!lot) return unknown();
          // Inverse futures quantities are USD face value, not base coins.
          return known(lot.minNotionalValue ?? (category === 'spot' ? lot.minOrderAmt : undefined),
            lot.minOrderQty ?? (category === 'spot' ? lot.basePrecision : undefined),
            category === 'spot' ? lot.basePrecision : lot.qtyStep, 1, category === 'inverse', undefined,
            lot.maxLimitOrderQty ?? lot.maxOrderQty);
        }; break;
      }
      case 'bitget':
        if (method !== 'GET' || path !== '/api/v2/mix/market/contracts' || !code(payload.code, '00000')) continue;
        if (!(name === 'bitgetinverse' ? ['coin-futures', 'scoin-futures'] : name === 'bitgetusdc' ?
          ['usdc-futures', 'susdc-futures'] : ['usdt-futures', 'susdt-futures']).includes(url.searchParams.get('productType'))) continue;
        rows = array(payload.data);
        rule = (row) => known(row.minTradeUSDT, row.minTradeNum, row.sizeMultiplier, 1, name === 'bitgetinverse', undefined,
          row.maxOrderQty ?? row.maxTradeNum); break;
      case 'bitrue':
        if (method !== 'GET' || path !== '/fapi/v1/contracts' || !Array.isArray(payload)) continue;
        rows = payload;
        rule = (row) => known(row.minOrderMoney, positive(row.minOrderVolume) * positive(row.multiplier), row.multiplier,
          1, false, undefined, positive(row.maxLimitVolume) * positive(row.multiplier)); break;
      case 'bingx':
        if (method !== 'GET' || path !== '/openApi/swap/v2/quote/contracts' || !code(payload.code)) continue;
        rows = array(payload.data);
        rule = (row) => known(row.tradeMinUSDT, row.tradeMinQuantity ?? row.tradeMinLimit,
          decimalStep(row.quantityPrecision)); break;
      case 'orderly': case 'woo':
        if (method !== 'GET' || path !== '/v1/public/info' || payload.success !== true) continue;
        rows = array(family === 'woo' ? payload.rows : payload.data?.rows);
        match = (row) => typeof row.symbol === 'string' && (family === 'woo' ?
          row.symbol === `PERP_${market.base}_${market.quote}` :
          row.symbol.replace(/^PERP_([^_]+)_/, '$1') === market.symbol);
        rule = (row) => known(row.min_notional, row.base_min, row.base_tick, 1, false, undefined, row.base_max); break;
      case 'lighter':
        if (method !== 'GET' || !['/api/v1/orderBookDetails', '/api/v1/orderBooks'].includes(path) || !code(payload.code, '200')) continue;
        rows = array(payload.order_book_details ?? payload.order_books);
        match = (row) => matches(row, market, 'symbol') && String(row.market_id) === market.id;
        rule = (row) => known(row.min_quote_amount, row.min_base_amount,
          decimalStep(row.supported_size_decimals)); break;
      case 'paradex':
        if (method !== 'GET' || path !== '/v1/markets' || payload.error || payload.error_code) continue;
        rows = array(payload.results);
        match = (row) => matches(row, market, 'symbol', 'market');
        // min_notional is monetary; the similarly named legacy min_order_size
        // must never be mistaken for monetary metadata without this field.
        rule = (row) => known(row.min_notional, row.order_size_increment ?? row.step_size,
          row.order_size_increment ?? row.step_size); break;
      case 'hyperliquid': {
        if (method !== 'POST' || path !== '/info' || !Array.isArray(payload)) continue;
        let request;
        try { request = JSON.parse(record.requestBody); } catch { continue; }
        if (request?.type !== 'metaAndAssetCtxs') continue;
        rows = array(payload[0]?.universe);
        match = (row) => row.name === market.id || (typeof row.name === 'string' && request.dex && `${request.dex}:${row.name}` === market.id);
        // Official MinTradeNtl defines $10 for perps; never apply it to spot.
        rule = (row) => known(10, decimalStep(row.szDecimals), decimalStep(row.szDecimals)); break;
      }
      case 'okx':
        if (method !== 'GET' || path !== '/api/v5/public/instruments' || !code(payload.code)) continue;
        rows = array(payload.data); match = (row) => matches(row, market, 'instId');
        rule = (row) => row.ctType === 'linear' && row.instId.endsWith('-SWAP') ?
          known(0, row.minSz, row.lotSz, row.ctVal, false, undefined,
            smallerMaximum(row.maxLmtSz, row.maxMktSz, row.maxIcebergSz, row.maxStopSz, row.maxTriggerSz, row.maxTwapSz)) : unknown(); break;
      case 'blofin':
        if (method !== 'GET' || path !== '/api/v1/market/instruments' || !code(payload.code)) continue;
        rows = array(payload.data); match = (row) => matches(row, market, 'instId');
        rule = (row) => row.contractType === 'linear' ? known(0, row.minSize, row.lotSize, row.contractValue,
          false, undefined, smallerMaximum(row.maxLimitSize, row.maxMarketSize)) : unknown(); break;
      case 'apexomni':
        if (method !== 'GET' || path !== '/api/v3/symbols' || !code(payload.code)) continue;
        rows = array(payload.data?.contractConfig?.perpetualContract);
        match = (row) => matches(row, market, 'symbol', 'crossSymbolName');
        rule = (row) => known(0, row.minOrderSize, row.stepSize, 1, false, undefined, row.maxOrderSize); break;
      case 'backpack':
        if (method !== 'GET' || path !== '/api/v1/markets' || !Array.isArray(payload)) continue;
        rows = payload;
        rule = (row) => known(0, row.minQuantity ?? row.filters?.quantity?.minQuantity,
          row.quantityIncrement ?? row.filters?.quantity?.stepSize, 1, false, undefined,
          row.maxQuantity ?? row.filters?.quantity?.maxQuantity); break;
      case 'kraken':
        if (method !== 'GET' || path !== '/derivatives/api/v3/instruments' || payload.result !== 'success') continue;
        rows = array(payload.instruments);
        rule = (row) => typeof row.symbol === 'string' && row.symbol.startsWith('PF_') &&
          decimalStep(row.contractValueTradePrecision, true) ?
          known(0, decimalStep(row.contractValueTradePrecision, true), decimalStep(row.contractValueTradePrecision, true), row.contractSize) : unknown(); break;
      case 'krakenspot':
        if (method !== 'GET' || path !== '/0/public/AssetPairs' || !Array.isArray(payload.error) || payload.error.length ||
          !payload.result || typeof payload.result !== 'object' || Array.isArray(payload.result)) continue;
        rows = Object.entries(payload.result).filter(([id]) => !id.endsWith('.d')).map(([, row]) => row);
        match = (row) => row.wsname === market.symbol;
        rule = (row) => market.id === `${market.base}/${market.quote}` &&
          row.status === 'online' && row.lot_multiplier === 1 ?
          known(row.costmin, row.ordermin, decimalStep(row.lot_decimals)) : unknown(); break;
      case 'kucoin':
        if (method !== 'GET' || path !== '/api/v1/contracts/active' || !code(payload.code, '200000')) continue;
        rows = array(payload.data);
        rule = (row) => row.isInverse === false ?
          known(0, row.minOrderQty ?? row.lotSize, row.lotSize, row.multiplier,
            false, undefined, row.maxOrderQty) : unknown(); break;
      case 'mexc':
        if (method !== 'GET' || path !== '/api/v1/contract/detail' || payload.success !== true || !code(payload.code)) continue;
        rows = array(payload.data);
        rule = (row) => known(0, row.minVol, row.volUnit, row.contractSize, false, undefined, row.maxVol); break;
      case 'weex':
        if (method !== 'GET' || path !== '/capi/v3/market/exchangeInfo') continue;
        rows = array(payload.symbols);
        rule = (row) => known(0, row.minOrderSize, row.minOrderSize, 1, false, undefined, row.maxOrderSize); break;
      case 'polymarket':
        if (method !== 'GET') continue;
        if (url.hostname === 'clob.polymarket.com' && path === '/book') {
          rows = [payload]; match = (row) => row.asset_id === market.id && url.searchParams.get('token_id') === market.id;
          rule = (row) => known(0, row.min_order_size, 0.01);
        } else if (url.hostname === 'gamma-api.polymarket.com' && path === '/markets/keyset') {
          rows = array(payload.markets);
          match = (row) => {
            try { return array(JSON.parse(row.clobTokenIds)).includes(market.id); } catch { return false; }
          };
          rule = (row) => known(0, row.orderMinSize, 0.01);
        } else continue;
        break;
      case 'coinbase':
        if (method !== 'GET') continue;
        if (url.hostname === 'api.international.coinbase.com' && path === '/api/v1/instruments/' && Array.isArray(payload)) {
          rows = payload; match = (row) => `${row.symbol}-INTX` === market.symbol;
          rule = (row) => known(row.min_notional_value, row.min_quantity, row.base_increment);
        } else if (url.hostname === 'api.coinbase.com' && ['/api/v3/brokerage/products', '/api/v3/brokerage/market/products'].includes(path)) {
          rows = array(payload.products); match = (row) => matches(row, market, 'product_id');
          rule = (row) => {
            const future = row.product_type === 'FUTURE';
            return known(row.quote_min_size, row.base_min_size, row.base_increment,
              future && row.future_product_details?.contract_expiry_type === 'EXPIRING' ? row.future_product_details?.contract_size : 1,
              false, undefined, row.base_max_size);
          };
        } else continue;
        break;
      default: continue;
    }
    // Ignore other pages, but let a later matching row invalidate stale rules.
    const selected = rows.filter((row) => row && typeof row === 'object' && !Array.isArray(row) && match(row));
    if (selected.length) result = selected.length === 1 ? rule(selected[0]) : unknown();
  }
  return result;
}
