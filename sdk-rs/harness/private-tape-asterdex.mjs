// Deterministic native payloads from Aster's official V1/V3 documentation.
// The V3 demonstration signer is public documentation material, never a real account.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { Wallet } from 'ethers';
import { ASTER_PRIVATE_KEY, ASTER_SIGNER, ASTER_USER } from './asterdex-requests.mjs';

export const ASTER_EPOCH = 1760880000000;
export const ASTER_BUILDER = '0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca';
export const ASTER_READ_PRIVATE_KEY = `0x${'24'.repeat(32)}`;
export const ASTER_READ_SIGNER = new Wallet(ASTER_READ_PRIVATE_KEY).address;
const REST = 'https://fapi.asterdex.com';
const PUBLIC = 'wss://fstream.asterdex.com/stream';
const KEY = 'aster-documented-listen-key';
const PRIVATE = `wss://fstream.asterdex.com/ws/${KEY}`;

export const buildAsterTape = ({ v3 = false, native = false, secondary = false } = {}) => {
  const version = v3 ? 'v3' : 'v1';
  const options = v3
    ? {
        key: ASTER_SIGNER,
        secret: ASTER_PRIVATE_KEY,
        applicationId: ASTER_USER,
        extra: { asterdexApiVersion: 'v3', asterBuilderFeeRateMaker: '0.00002', asterBuilderFeeRateTaker: '0.00005' },
        ...(secondary ? { secondary_key: ASTER_READ_SIGNER, secondary_secret: ASTER_READ_PRIVATE_KEY } : {}),
        asterdex: {
          apiVersion: 'v3',
          builderFeeRateMaker: '0.00002',
          builderFeeRateTaker: '0.00005',
          ...(secondary ? { readKey: ASTER_READ_SIGNER, readSecret: ASTER_READ_PRIVATE_KEY } : {}),
        },
      }
    : { extra: { asterdexApiVersion: 'v1' }, asterdex: { apiVersion: 'v1' } };
  const records = [
    { t: 0, type: 'meta', epoch: ASTER_EPOCH, private: true },
    { t: 0, type: 'config', private: true, options },
  ];
  const http = (method, endpoint, query, payload, extras = {}) => {
    const path = endpoint === 'account' && !v3 ? '/fapi/v4/account' : `/fapi/${version}/${endpoint}`;
    const signed = !['time', 'exchangeInfo', 'ticker/24hr', 'openInterest', 'trades', 'klines', 'depth'].includes(endpoint);
    const code = v3 && method === 'POST' && endpoint === 'order';
    const credentials =
      v3 && signed
        ? `${code ? 'asterChain=Mainnet&' : ''}user=${ASTER_USER}&signer=${secondary && method === 'GET' ? ASTER_READ_SIGNER : ASTER_SIGNER}`
        : '';
    const fullQuery = [query, credentials].filter(Boolean).join('&');
    records.push({
      t: 0,
      type: 'http',
      method,
      url: `${REST}${path}${fullQuery ? `?${fullQuery}` : ''}`,
      status: 200,
      body: JSON.stringify(payload),
      ...extras,
    });
  };
  const ws = (t, url, data) => records.push({ t, type: 'ws', url, text: JSON.stringify(data) });
  const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
  const order = (id, overrides = {}) => ({
    symbol: 'BTCUSDT',
    orderId: id,
    clientOrderId: 'native-website',
    price: '90',
    origQty: '1',
    executedQty: '0',
    cummulativeQuoteQty: '0',
    status: 'NEW',
    timeInForce: 'GTC',
    type: 'LIMIT',
    side: 'BUY',
    stopPrice: '0',
    time: ASTER_EPOCH,
    updateTime: ASTER_EPOCH,
    positionSide: 'BOTH',
    closePosition: false,
    reduceOnly: false,
    workingType: 'CONTRACT_PRICE',
    ...overrides,
  });
  const event = (id, overrides = {}) => ({
    e: 'ORDER_TRADE_UPDATE',
    E: ASTER_EPOCH + 1000 + id,
    T: ASTER_EPOCH + 1000 + id,
    o: {
      s: 'BTCUSDT',
      c: 'native-website',
      S: 'BUY',
      o: 'LIMIT',
      f: 'GTC',
      q: '1',
      p: '90',
      ap: '0',
      sp: '0',
      x: 'NEW',
      X: 'NEW',
      i: id,
      l: '0',
      z: '0',
      L: '0',
      N: 'USDT',
      n: '0',
      T: ASTER_EPOCH + 1000 + id,
      t: 0,
      b: '0',
      a: '0',
      m: false,
      R: false,
      wt: 'CONTRACT_PRICE',
      ot: 'LIMIT',
      ps: 'BOTH',
      cp: false,
      rp: '0',
      ...overrides,
    },
  });
  http('GET', 'time', '', { serverTime: ASTER_EPOCH });
  http('GET', 'exchangeInfo', '', {
    symbols: [
      {
        symbol: 'BTCUSDT',
        status: 'TRADING',
        contractType: 'PERPETUAL',
        baseAsset: 'BTC',
        quoteAsset: 'USDT',
        marginAsset: 'USDT',
        pricePrecision: 2,
        quantityPrecision: 3,
        filters: [
          { filterType: 'PRICE_FILTER', tickSize: '0.10' },
          { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '2' },
          { filterType: 'MAX_NUM_ALGO_ORDERS', limit: 100 },
        ],
      },
    ],
  });
  http('GET', 'leverageBracket', '', [
    {
      symbol: 'BTCUSDT',
      brackets: [{ bracket: 1, initialLeverage: 125, notionalCap: 50000, notionalFloor: 0, maintMarginRatio: 0.004 }],
    },
  ]);
  http('GET', 'ticker/24hr', '', [
    { symbol: 'BTCUSDT', lastPrice: '100', priceChangePercent: '1', volume: '1000', quoteVolume: '100000' },
  ]);
  http('GET', 'openInterest', '', [{ symbol: 'BTCUSDT', openInterest: '1234.5' }]);
  http('GET', 'account', '', {
    accountAlias: 'documented-fixture',
    canTrade: true,
    totalWalletBalance: '1000',
    availableBalance: '900',
    totalInitialMargin: '80',
    totalOpenOrderInitialMargin: '20',
    totalUnrealizedProfit: '10',
    assets: [
      {
        asset: 'USDT',
        walletBalance: '1000',
        availableBalance: '900',
        initialMargin: '100',
        unrealizedProfit: '10',
        crossWalletBalance: '900',
        crossUnPnl: '10',
      },
    ],
    positions: [],
  });
  http('GET', 'positionSide/dual', '', { dualSidePosition: false });
  http('GET', 'positionRisk', '', [
    {
      symbol: 'BTCUSDT',
      positionAmt: '1',
      entryPrice: '90',
      breakEvenPrice: '90',
      markPrice: '100',
      unRealizedProfit: '10',
      liquidationPrice: '50',
      leverage: '10',
      maxNotionalValue: '1000000',
      marginType: 'CROSS',
      isolatedMargin: '0',
      isolatedWallet: '0',
      positionSide: 'BOTH',
      notional: '100',
      updateTime: ASTER_EPOCH,
    },
  ]);
  http('GET', 'openOrders', '', [order(1001)]);
  http('POST', 'listenKey', '', { listenKey: KEY });
  ws(100, PUBLIC, {
    stream: '!ticker@arr',
    data: [{ e: '24hrTicker', E: ASTER_EPOCH, s: 'BTCUSDT', c: '100', P: '1', v: '1000', q: '100000' }],
  });
  ws(200, PRIVATE, {
    e: 'ACCOUNT_UPDATE',
    E: ASTER_EPOCH + 200,
    T: ASTER_EPOCH + 200,
    a: {
      m: 'ORDER',
      B: [{ a: 'USDT', wb: '1000', cw: '900', bc: '0' }],
      P: [{ s: 'BTCUSDT', pa: '1', ep: '90', cr: '0', up: '10', mt: 'cross', iw: '0', ps: 'BOTH' }],
    },
  });
  command(600, 'placeOrders', {
    orders: [
      {
        symbol: 'BTCUSDT',
        type: 'limit',
        side: 'buy',
        amount: 0.003,
        price: 91,
        ...(v3 ? { stpMode: 'EXPIRE_BOTH' } : {}),
      },
    ],
  });
  const attribution = v3 ? `&stpMode=EXPIRE_BOTH&builder=${ASTER_BUILDER}&feeRate=0.00005` : '';
  http(
    'POST',
    'order',
    `symbol=BTCUSDT&side=BUY&type=LIMIT${attribution}&quantity=0.003&timeInForce=GTC&price=91`,
    order(1002, { origQty: '0.003', price: '91' }),
  );
  ws(1000, PRIVATE, event(1002, { q: '0.003', p: '91' }));
  command(1400, 'amendOrder', { id: '1002', price: 92 });
  http(
    'DELETE',
    'order',
    'symbol=BTCUSDT&orderId=1002',
    order(1002, { origQty: '0.003', price: '91', status: 'CANCELED' }),
  );
  http(
    'POST',
    'order',
    `symbol=BTCUSDT&side=BUY&type=LIMIT${v3 ? `&builder=${ASTER_BUILDER}&feeRate=0.00005` : ''}&quantity=0.003&timeInForce=GTC&price=92`,
    order(1003, { origQty: '0.003', price: '92' }),
  );
  ws(1800, PRIVATE, event(1002, { q: '0.003', p: '91', X: 'CANCELED', x: 'CANCELED' }));
  ws(2200, PRIVATE, event(1003, { q: '0.003', p: '92' }));
  command(2600, 'cancelOrders', { ids: ['1003'] });
  http(
    'DELETE',
    'order',
    'symbol=BTCUSDT&orderId=1003',
    order(1003, { origQty: '0.003', price: '92', status: 'CANCELED' }),
  );
  ws(3000, PRIVATE, event(1003, { q: '0.003', p: '92', X: 'CANCELED', x: 'CANCELED' }));
  const fills = [
    {
      symbol: 'BTCUSDT',
      id: 3,
      orderId: 1001,
      price: '90',
      qty: '0.3',
      side: 'BUY',
      positionSide: 'BOTH',
      realizedPnl: '15',
      commission: '0.01',
      commissionAsset: 'USDT',
      maker: false,
      time: ASTER_EPOCH + 3000,
    },
    {
      symbol: 'BTCUSDT',
      id: 1,
      orderId: 1001,
      price: '89',
      qty: '0.4',
      side: 'BUY',
      positionSide: 'BOTH',
      realizedPnl: '10',
      commission: '0.02',
      commissionAsset: 'USDT',
      maker: true,
      time: ASTER_EPOCH + 1000,
    },
    {
      symbol: 'BTCUSDT',
      id: 2,
      orderId: 1001,
      price: '90',
      qty: '0.3',
      side: 'BUY',
      positionSide: 'BOTH',
      realizedPnl: '12',
      commission: '0.01',
      commissionAsset: 'USDT',
      maker: false,
      time: ASTER_EPOCH + 2000,
    },
  ];
  http('GET', 'userTrades', 'symbol=BTCUSDT&limit=500', fills);
  command(3400, 'fetchFills', { symbol: 'BTCUSDT', limit: 500 });
  ws(3800, PRIVATE, event(1001, { x: 'TRADE', X: 'PARTIALLY_FILLED', l: '0.4', z: '0.4', L: '89', ap: '89', t: 1 }));
  ws(4200, PRIVATE, event(1001, { x: 'TRADE', X: 'FILLED', l: '0.6', z: '1', L: '90', ap: '89.6', t: 2 }));
  if (native) {
    command(4600, 'fetchStpMode');
    http('GET', 'stpMode', '', { stpMode: 'EXPIRE_MAKER' });
    command(5000, 'setStpMode', { mode: 'EXPIRE_TAKER' });
    http('POST', 'stpMode', 'stpMode=EXPIRE_TAKER', { code: 200, msg: 'success' });
    command(5400, 'fetchOrderHistory', { symbol: 'BTCUSDT', limit: 10 });
    http('GET', 'allOrders', 'symbol=BTCUSDT&limit=10', [
      { ...order(1001, { status: 'FILLED', executedQty: '1', avgPrice: '89.6' }), origType: 'LIMIT' },
    ]);
    command(5800, 'fetchIncome', { symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', limit: 10 });
    http('GET', 'income', 'symbol=BTCUSDT&incomeType=FUNDING_FEE&limit=10', [
      {
        symbol: 'BTCUSDT',
        incomeType: 'FUNDING_FEE',
        income: '-0.25',
        asset: 'USDT',
        time: ASTER_EPOCH + 1000,
        tranId: 9001,
        tradeId: '',
      },
    ]);
    command(6200, 'fetchPublicTrades', { symbol: 'BTCUSDT', limit: 10 });
    http('GET', 'trades', 'symbol=BTCUSDT&limit=10', [
      { id: 28457, price: '89', qty: '0.4', quoteQty: '35.6', time: ASTER_EPOCH, isBuyerMaker: true },
    ]);
    command(6600, 'fetchCandles', { symbol: 'BTCUSDT', interval: '1m', limit: 1 });
    http('GET', 'klines', 'symbol=BTCUSDT&interval=1m&limit=1', [
      [ASTER_EPOCH, '89', '91', '88', '90', '10', ASTER_EPOCH + 59999, '900', 3, '5', '450', '0'],
    ]);
    command(7000, 'placeOrders', {
      orders: [{ symbol: 'BTCUSDT', type: 'trailing_stop_market', side: 'sell', amount: 1, price: 0.5 }],
    });
    command(7400, 'placeOrders', {
      orders: [
        { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 1, price: 90, clientOrderId: 'caller-not-supported' },
      ],
    });
    command(7800, 'fetchAccount');
    command(8200, 'fetchLeverage', { symbol: 'BTCUSDT' });
    command(8600, 'fetchOrderBook', { symbol: 'BTCUSDT', limit: 100 });
    http('GET', 'depth', 'symbol=BTCUSDT&limit=100', {
      lastUpdateId: 1027024,
      E: ASTER_EPOCH,
      T: ASTER_EPOCH,
      bids: [
        ['88', '3'],
        ['89', '2'],
      ],
      asks: [
        ['92', '5'],
        ['91', '4'],
      ],
    });
  }
  const final = order(1004, { price: '93', origQty: '0.004' });
  http('GET', 'openOrders', '', [final], { availableAfter: 10000 });
  ws(10000, PRIVATE, event(1004, { p: '93', q: '0.004' }));
  ws(11000, PUBLIC, {
    stream: '!markPrice@arr@1s',
    data: [
      {
        e: 'markPriceUpdate',
        E: ASTER_EPOCH + 11000,
        s: 'BTCUSDT',
        p: '100',
        i: '99.9',
        r: '0.0001',
        T: ASTER_EPOCH + 28800000,
      },
    ],
  });
  return records;
};

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({
    options: {
      out: { type: 'string', default: 'sdk-rs/harness/tapes/asterdex-private.jsonl' },
      v3: { type: 'boolean', default: false },
      native: { type: 'boolean', default: false },
      secondary: { type: 'boolean', default: false },
    },
  });
  const records = buildAsterTape(values);
  mkdirSync(dirname(values.out), { recursive: true });
  writeFileSync(values.out, `${records.map((record) => JSON.stringify(record)).join('\n')}\n`);
  console.log(JSON.stringify({ out: values.out, records: records.length, version: values.v3 ? 'v3' : 'v1' }));
}
