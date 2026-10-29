// Classic COIN-FUTURES schemas adapted to BTCUSD and ETHUSD. Fake credentials only.
// Official payload definitions:
// https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market
// https://www.bitget.com/docs/catalog/classic-contract-position/classic-contract-position
// https://www.bitget.com/docs/catalog/classic-contract-account/classic-contract-account
// https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade
// Native units follow BitgetInverseExchange: order amount/filled remain native;
// remaining and position contracts divide by their TS reference prices.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

export function buildBitgetInverseTape() {
  const REST = 'https://api.bitget.com';
  const PUBLIC = 'wss://ws.bitget.com/v2/ws/public';
  const PRIVATE = 'wss://ws.bitget.com/v2/ws/private';
  const records = [{ t: 0, type: 'meta', private: true, epoch: 1760880000000 }];
  const http = (method, path, query, data, requestBody, code = '00000') => {
    const record = {
      t: 0, type: 'http', method,
      url: `${REST}${path}${query ? `?${query}` : ''}`,
      ...(requestBody === undefined ? {} : { requestBody }),
      status: 200,
      body: JSON.stringify({ code, msg: code === '00000' ? 'success' : 'Classic Account mode', data }),
    };
    records.push(record);
    return record;
  };
  const ws = (t, url, message) => records.push({ t, type: 'ws', url, text: JSON.stringify(message) });
  const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
  const contract = (base, pricePlace, maxTradeNum) => ({
    symbol: `${base}USD`, baseCoin: base, quoteCoin: 'USD', symbolStatus: 'normal',
    pricePlace, priceEndStep: '1', sizeMultiplier: '1', minTradeNum: '1',
    maxTradeNum, minLever: '1', maxLever: '125',
  });
  http('GET', '/api/v2/mix/market/contracts', 'productType=coin-futures', [
    contract('BTC', '1', '2000'), contract('ETH', '2', '3000'),
  ]);
  http('GET', '/api/v2/mix/market/tickers', 'productType=coin-futures', [
    { symbol: 'BTCUSD', bidPr: '49999.9', askPr: '50000.1', lastPr: '50000', indexPrice: '49990',
      changeUtc24h: '0.01', baseVolume: '1000', quoteVolume: '50000000', fundingRate: '0.0001' },
    { symbol: 'ETHUSD', bidPr: '2499.99', askPr: '2500.01', lastPr: '2500', indexPrice: '2499',
      changeUtc24h: '-0.02', baseVolume: '2000', quoteVolume: '5000000', fundingRate: '0.0002' },
  ]);
  // Shared TS startup still probes account type even though inverse cannot route
  // UTA. Rust directly selects Classic; this reusable GET requires no writes.
  http('GET', '/api/v3/account/settings', '', null, undefined, '40084');
  const position = (base, total, entryPrice, markPrice, upnl) => ({
    symbol: `${base}USD`, instId: `${base}USD`, marginCoin: base, holdSide: 'long',
    total, openPriceAvg: entryPrice, markPrice, leverage: '20', unrealizedPL: upnl,
    achievedProfits: '0.2', totalFee: '-0.02', deductedFee: '0.01',
    liquidationPrice: '1000', marginSize: '0.1', marginMode: 'crossed',
    posMode: 'one_way_mode', uTime: '1760880000000',
  });
  http('GET', '/api/v2/mix/position/all-position', 'productType=coin-futures&marginCoin=BTC', [
    position('BTC', '1000', '45000', '50000', '0.01'),
  ]);
  http('GET', '/api/v2/mix/position/all-position', 'productType=coin-futures&marginCoin=ETH', [
    position('ETH', '333', '2000', '2500', '0.02'),
  ]);
  http('GET', '/api/v2/mix/account/accounts', 'productType=coin-futures', [
    { marginCoin: 'BTC', unionTotalMargin: '2', accountEquity: '2', unionAvailable: '1.5',
      available: '1.5', unrealizedPL: '0.1', usdtEquity: '100000' },
    // Exercise ticker mark fallback rather than an explicit USD valuation.
    { marginCoin: 'ETH', accountEquity: '3', available: '2', unrealizedPL: '0.2', usdtEquity: '0' },
  ]);
  const normal = (id, symbol, size, price, clientOid, baseVolume = '0') => ({
    orderId: id, clientOid, symbol, size, baseVolume, price, side: 'buy',
    orderType: 'limit', status: 'live', force: 'gtc', reduceOnly: 'NO',
    posMode: 'one_way_mode', cTime: '1760880000000', uTime: '1760880000000',
  });
  const resting = [
    normal('inverse-rest-btc', 'BTCUSD', '1000', '50000', 'resting-btc', '100'),
    normal('inverse-rest-eth', 'ETHUSD', '500', '2500', 'resting-eth'),
  ];
  // Rust reads pending orders unfiltered at limit=200; the TS reference reads
  // each documented status at the Classic maximum of 100. Same venue rows.
  const tagged = (impl) => Object.assign(records.at(-1), { impl });
  http('GET', '/api/v2/mix/order/orders-pending', 'productType=coin-futures&limit=200', {
    entrustedList: resting, endId: '',
  });
  tagged('rust');
  for (const status of ['live', 'partially_filled']) {
    http('GET', '/api/v2/mix/order/orders-pending', `productType=coin-futures&status=${status}&limit=100`, {
      entrustedList: resting.filter((row) => row.status === status), endId: '',
    });
    tagged('ts');
  }
  for (const planType of ['profit_loss', 'track_plan']) {
    for (const [impl, limit] of [['rust', 200], ['ts', 100]]) {
      http('GET', '/api/v2/mix/order/orders-plan-pending', `productType=coin-futures&planType=${planType}&limit=${limit}`, {
        entrustedList: [], endId: '',
      });
      tagged(impl);
    }
  }
  for (const base of ['BTC', 'ETH']) {
    http('GET', '/api/v2/mix/account/account', `symbol=${base}USD&productType=coin-futures&marginCoin=${base}`, {
      symbol: `${base}USD`, posMode: 'one_way_mode', marginMode: 'crossed', leverage: '20',
    });
  }
  const orderEvent = (t, rows) => ws(t, PRIVATE, {
    arg: { instType: 'COIN-FUTURES', channel: 'orders', instId: 'default' },
    action: 'snapshot', data: rows,
  });
  ws(0, PRIVATE, { event: 'login', code: '0', connId: 'inverse-documented-extra-field' });
  ws(100, PUBLIC, {
    arg: { instType: 'COIN-FUTURES', channel: 'ticker', instId: 'BTCUSD' },
    action: 'snapshot', data: [{ instId: 'BTCUSD', lastPr: '50000', markPrice: '50000', bidPr: '49999.9', askPr: '50000.1' }],
  });
  ws(200, PUBLIC, {
    arg: { instType: 'COIN-FUTURES', channel: 'ticker', instId: 'ETHUSD' },
    action: 'snapshot', data: [{ instId: 'ETHUSD', lastPr: '2500', markPrice: '2500', bidPr: '2499.99', askPr: '2500.01' }],
  });
  command(600, 'placeOrders', { orders: [
    { symbol: 'BTCUSD', type: 'limit', side: 'buy', amount: 300, price: 49000, clientOrderId: 'caller:inverse-btc' },
    { symbol: 'ETHUSD', type: 'limit', side: 'buy', amount: 400, price: 2400, clientOrderId: 'caller:inverse-eth' },
  ] });
  for (const [base, id, size, price, caller] of [
    ['BTC', 'inverse-btc-created', '300', '49000', 'caller:inverse-btc'],
    ['ETH', 'inverse-eth-created', '400', '2400', 'caller:inverse-eth'],
  ]) {
    const batch = http('POST', '/api/v2/mix/order/batch-place-order', '', {
      successList: [{ orderId: id, clientOid: caller }], failureList: [],
    }, {
      symbol: `${base}USD`, productType: 'coin-futures', marginCoin: base, marginMode: 'crossed',
      orderList: [{ size, price, side: 'buy', orderType: 'limit', force: 'gtc', reduceOnly: 'NO', clientOid: caller }],
    });
    // The shared TS formatter sends enum "cross". The documented native wire
    // value remains "crossed"; only this declared TS request differs.
    batch.tsRequestBody = { ...batch.requestBody, marginMode: 'cross' };
  }
  const createdBtc = normal('inverse-btc-created', 'BTCUSD', '300', '49000', 'caller:inverse-btc');
  const createdEth = normal('inverse-eth-created', 'ETHUSD', '400', '2400', 'caller:inverse-eth');
  orderEvent(1000, [createdBtc, createdEth]);
  command(1400, 'amendOrder', { id: createdBtc.orderId, price: 48000, amount: 350 });
  http('POST', '/api/v2/mix/order/modify-order', '', { orderId: 'inverse-btc-amended' }, {
    orderId: createdBtc.orderId, symbol: 'BTCUSD', productType: 'coin-futures',
    newPrice: '48000', newSize: '350', newClientOid: 'generated-inverse-amend',
  });
  const amended = normal('inverse-btc-amended', 'BTCUSD', '350', '48000', 'generated-inverse-amend');
  orderEvent(1800, [{ ...createdBtc, status: 'canceled' }]);
  orderEvent(2000, [amended]);
  command(2400, 'cancelOrders', { ids: [amended.orderId, createdEth.orderId] });
  for (const [base, id] of [['BTC', amended.orderId], ['ETH', createdEth.orderId]]) {
    http('POST', '/api/v2/mix/order/batch-cancel-orders', '', {
      successList: [{ orderId: id }], failureList: [],
    }, { symbol: `${base}USD`, orderIdList: [{ orderId: id }], marginCoin: base, productType: 'coin-futures' });
  }
  orderEvent(2800, [{ ...amended, status: 'canceled' }, { ...createdEth, status: 'canceled' }]);
  // A delayed execution for the resting order retains the native100 quantity;
  // displaying its remaining quantity still divides by the limit price.
  orderEvent(3200, [{
    ...resting[0], status: 'partially_filled', tradeId: 'inverse-native-execution',
    fillPrice: '50000', priceAvg: '50000',
  }]);
  // Leave nonempty native-sized resting orders and allow queued TS 3-RPS
  // reconciliation to finish. Native unit assertions also run independently.
  // Corrected inverse WS positions/accounts are verified by native tests; the
  // shared TS handlers contain known unit bugs and are excluded from this tape.
  orderEvent(6000, resting);
  orderEvent(10000, resting);
  return records;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const { values } = parseArgs({
    options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bitgetinverse-private.jsonl' } },
  });
  const records = buildBitgetInverseTape();
  mkdirSync(dirname(values.out), { recursive: true });
  writeFileSync(values.out, `${records.map((record) => JSON.stringify(record)).join('\n')}\n`);
  console.log(JSON.stringify({ out: values.out, records: records.length, seconds: 10 }));
}
