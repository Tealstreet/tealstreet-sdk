// Fake credentials only. UTA trailing lifecycle is verified through documented
// REST snapshots, not inferred REST-shaped strategy-order WebSocket messages.
// https://www.bitget.com/docs/catalog/account/assets-balance#get-financial-records
// https://www.bitget.com/docs/catalog/trading/strategy-trading
// https://www.bitget.com/docs/uta/websocket/private/Strategy-Order-Channel.md
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

export const UTA_EPOCH = 1760880000000;

export function buildBitgetUtaTape({ usdc = false } = {}) {
  const coin = usdc ? 'USDC' : 'USDT';
  const symbol = `BTC${coin}`;
  const category = `${coin}-FUTURES`;
  const product = category.toLowerCase();
  const rest = 'https://api.bitget.com';
  const pub = 'wss://ws.bitget.com/v2/ws/public';
  const priv = 'wss://ws.bitget.com/v3/ws/private';
  const records = [{ type: 'meta', private: true, epoch: UTA_EPOCH }];
  const http = (method, path, query, data, requestBody, code = '00000') =>
    records.push({
      t: 0,
      type: 'http',
      method,
      url: `${rest}${path}${query ? `?${query}` : ''}`,
      ...(requestBody === undefined ? {} : { requestBody }),
      status: 200,
      body: JSON.stringify({ code, msg: code === '00000' ? 'success' : 'invalid qty', data }),
    });
  const ws = (t, url, message) => records.push({ t, type: 'ws', url, text: JSON.stringify(message) });
  const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
  const nativeParams = (activationPrice, trailVariance) => ({
    activationPrice,
    activationType: 'market',
    trailType: 'distance',
    trailVariance,
    preOrderType: 'market',
  });
  const trailingRow = (orderId, qty = '0.01', activation = '95', variance = '7', status = 'pending') => ({
    orderId,
    category,
    symbol,
    type: 'trailing_stop',
    qty,
    side: 'sell',
    posSide: 'long',
    status,
    // Bitget names the same price-distance mode "spread" in GET responses
    // and "distance" in POST placement parameters.
    trailingStopParams: { ...nativeParams(activation, variance), trailType: 'spread' },
    createdTime: String(UTA_EPOCH),
    updatedTime: String(UTA_EPOCH),
  });
  const placeBody = (qty, activation, variance) => ({
    category,
    symbol,
    type: 'trailing_stop',
    side: 'sell',
    posSide: 'long',
    qty,
    marginMode: 'crossed',
    trailingStopParams: [nativeParams(activation, variance)],
  });
  http('GET', '/api/v2/mix/market/contracts', `productType=${product}`, [
    {
      symbol,
      baseCoin: 'BTC',
      quoteCoin: coin,
      symbolStatus: 'normal',
      pricePlace: '1',
      priceEndStep: '1',
      sizeMultiplier: '0.001',
      minTradeNum: '0.001',
      maxOrderQty: '120',
      maxLever: '125',
      minLever: '1',
    },
  ]);
  http('GET', '/api/v2/mix/market/tickers', `productType=${product}`, [
    {
      symbol,
      bidPr: '99.9',
      askPr: '100.1',
      lastPr: '100',
      indexPrice: '99',
      changeUtc24h: '0.01',
      baseVolume: '1000',
      quoteVolume: '100000',
      fundingRate: '0.0001',
    },
  ]);
  http('GET', '/api/v3/account/settings', '', {
    holdMode: 'hedge_mode',
    symbolConfigList: [
      {
        category,
        symbol,
        marginMode: 'crossed',
        leverage: '20',
      },
    ],
  });
  http('GET', '/api/v3/position/current-position', `category=${category}`, { list: [], cursor: '' });
  http('GET', '/api/v3/account/assets', '', { accountEquity: '1000', effEquity: '900', unrealisedPnl: '0' });
  http('GET', '/api/v3/trade/unfilled-orders', `category=${category}&limit=100`, { list: [], cursor: '' });
  const tpsl = {
    orderId: 'existing-stop',
    type: 'tpsl',
    category,
    symbol,
    qty: '0.01',
    side: 'sell',
    posSide: 'long',
    stopLoss: '90',
    slTriggerBy: 'market',
    slOrderType: 'market',
    status: 'pending',
    createdTime: String(UTA_EPOCH),
    updatedTime: String(UTA_EPOCH),
  };
  http('GET', '/api/v3/trade/unfilled-strategy-orders', `category=${category}&type=tpsl`, [tpsl]);
  const trailingSnapshot = (afterCommand, rows) => {
    http('GET', '/api/v3/trade/unfilled-strategy-orders', `category=${category}&type=trailing_stop`, rows);
    records.at(-1).afterCommand = afterCommand;
  };
  trailingSnapshot(0, [trailingRow('trail-rest'), trailingRow('trail-disappears', '0.01', '94', '2')]);
  trailingSnapshot(2, [trailingRow('trail-rest'), trailingRow('trail-created', '0.01', '95', '5')]);
  trailingSnapshot(3, [trailingRow('trail-rest'), trailingRow('trail-restored', '0.01', '95', '5')]);
  trailingSnapshot(4, [
    trailingRow('trail-restored', '0.01', '95', '5'),
    trailingRow('trail-amended', '0.02', '95', '7'),
  ]);
  trailingSnapshot(5, [trailingRow('trail-amended', '0.02', '95', '7'), trailingRow('trail-final', '0.03', '96', '3')]);
  ws(0, priv, { event: 'login', code: '0' });
  const bill = (id, type, amount, fee) => ({
    id,
    category,
    symbol,
    coin,
    type,
    amount,
    fee,
    balance: '1000',
    ts: String(UTA_EPOCH - 1000),
    positionType: 'crossed',
    positionAmount: '0.001',
    positionBalance: '0.001',
  });
  const funding = bill('9007199254740993', 'CONTRACT_MAIN_SETTLE_FEE_USER_OUT', '-2', '0');
  const range = `category=${category}&limit=2&startTime=${UTA_EPOCH - 10000}&endTime=${UTA_EPOCH}`;
  http('GET', '/api/v3/account/financial-records', range, {
    list: [funding, bill('2', 'CLOSE_LONG', '10', '-0.1')],
    cursor: 'next-ledger',
  });
  http('GET', '/api/v3/account/financial-records', `${range}&cursor=next-ledger`, {
    list: [funding, bill('3', 'TRANSFER_IN', '5', '0'), bill('4', 'ORDER_PLF_FEE_OUT', '-0.2', '-0.2')],
    cursor: '',
  });
  command(100, 'fetchIncome', { from: UTA_EPOCH - 10000, to: UTA_EPOCH, limit: 2 });
  command(500, 'placeOrders', {
    orders: [
      { symbol, type: 'trailing_stop_market', side: 'sell', amount: 0.01, price: 95, slTriggerType: 'last_price' },
    ],
  });
  http('POST', '/api/v3/trade/place-strategy-order', '', { orderId: 'trail-created' }, placeBody('0.01', '95', '5'));
  command(900, 'amendOrder', { id: 'trail-created', price: 96 });
  http('POST', '/api/v3/trade/cancel-strategy-order', '', null, { orderId: 'trail-created' });
  http('POST', '/api/v3/trade/place-strategy-order', '', null, placeBody('0.01', '96', '4'), '43023');
  http('POST', '/api/v3/trade/place-strategy-order', '', { orderId: 'trail-restored' }, placeBody('0.01', '95', '5'));
  // Published strategy-order schema covers TP/SL, not trailingStopParams.
  ws(1100, priv, {
    action: 'snapshot',
    arg: { instType: 'UTA', topic: 'strategy-order', symbol: 'default' },
    data: [
      {
        ...tpsl,
        category: category.toLowerCase(),
        side: 'buy',
        reduceOnly: 'no',
        triggerType: 'stopLoss',
        updatedTime: String(UTA_EPOCH + 1100),
      },
    ],
    ts: UTA_EPOCH + 1100,
  });
  ws(1300, pub, {
    arg: { instType: category, channel: 'ticker', instId: symbol },
    action: 'snapshot',
    data: [{ instId: symbol, lastPr: '110', bidPr: '109.9', askPr: '110.1' }],
  });
  command(1500, 'amendOrder', { id: 'trail-rest', amount: 0.02 });
  http('POST', '/api/v3/trade/cancel-strategy-order', '', null, { orderId: 'trail-rest' });
  http('POST', '/api/v3/trade/place-strategy-order', '', { orderId: 'trail-amended' }, placeBody('0.02', '95', '7'));
  command(1900, 'cancelOrders', { ids: ['trail-restored'] });
  http('POST', '/api/v3/trade/cancel-strategy-order', '', null, { orderId: 'trail-restored' });
  const fill = {
    category,
    symbol,
    execId: 'execution-one',
    orderId: 'external-filled',
    side: 'buy',
    execPrice: '110',
    execQty: '0.004',
  };
  ws(2300, priv, { arg: { instType: 'UTA', topic: 'fill' }, data: [fill, fill] });
  return records;
}

export function buildBitgetClassicTrailingTape({ usdc = false } = {}) {
  const coin = usdc ? 'USDC' : 'USDT';
  const symbol = `BTC${coin}`;
  const category = `${coin}-FUTURES`;
  const productType = category.toLowerCase();
  const rest = 'https://api.bitget.com';
  const priv = 'wss://ws.bitget.com/v2/ws/private';
  const records = buildBitgetUtaTape({ usdc }).filter(
    (r) => r.type === 'meta' || (r.type === 'http' && new URL(r.url).pathname.startsWith('/api/v2/mix/market/')),
  );
  const http = (method, path, query, data, requestBody, code = '00000') =>
    records.push({
      t: 0,
      type: 'http',
      method,
      url: `${rest}${path}${query ? `?${query}` : ''}`,
      ...(requestBody === undefined ? {} : { requestBody }),
      status: 200,
      body: JSON.stringify({ code, msg: 'success', data }),
    });
  const ws = (t, url, message) => records.push({ t, type: 'ws', url, text: JSON.stringify(message) });
  const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
  const row = (orderId, size = '0.01', triggerPrice = '95', callbackRatio = '5') => ({
    orderId,
    symbol,
    planType: 'track_plan',
    size,
    triggerPrice,
    callbackRatio,
    triggerType: 'fill_price',
    planStatus: 'not_trigger',
    side: 'sell',
    posMode: 'one_way_mode',
    marginMode: 'crossed',
    orderType: 'market',
    cTime: String(UTA_EPOCH),
    uTime: String(UTA_EPOCH),
  });
  const strategy = (t, value) =>
    ws(t, priv, {
      arg: { instType: category, channel: 'orders-algo', instId: 'default' },
      action: 'snapshot',
      data: [value],
    });
  http('GET', '/api/v3/account/settings', '', null, undefined, '40084');
  http('GET', '/api/v2/mix/position/all-position', `productType=${productType}&marginCoin=${coin}`, []);
  http('GET', '/api/v2/mix/account/accounts', `productType=${productType}`, [
    { marginCoin: coin, unionTotalMargin: '1000', unionAvailable: '900', unrealizedPL: '0' },
  ]);
  http('GET', '/api/v2/mix/account/account', `symbol=${symbol}&productType=${productType}&marginCoin=${coin}`, {
    posMode: 'one_way_mode',
    marginMode: 'crossed',
    leverage: '20',
  });
  http('GET', '/api/v2/mix/order/orders-pending', `productType=${productType}&limit=200`, {
    entrustedList: [],
    endId: '',
  });
  http('GET', '/api/v2/mix/order/orders-plan-pending', `productType=${productType}&planType=profit_loss&limit=200`, {
    entrustedList: [],
    endId: '',
  });
  http('GET', '/api/v2/mix/order/orders-plan-pending', `productType=${productType}&planType=track_plan&limit=200`, {
    entrustedList: [row('classic-rest', '0.01', '97', '3')],
    endId: '',
  });
  ws(0, priv, { event: 'login', code: '0' });
  command(500, 'placeOrders', {
    orders: [
      { symbol, type: 'trailing_stop_market', side: 'sell', amount: 0.01, price: 95, slTriggerType: 'last_price' },
    ],
  });
  http(
    'POST',
    '/api/v2/mix/order/place-plan-order',
    '',
    { orderId: 'classic-created' },
    {
      symbol,
      productType,
      marginCoin: coin,
      clientOid: 'generated',
      reduceOnly: 'YES',
      triggerPrice: '95',
      triggerType: 'fill_price',
      size: '0.01',
      planType: 'track_plan',
      side: 'sell',
      orderType: 'market',
      marginMode: 'crossed',
      callbackRatio: '5',
    },
  );
  strategy(700, row('classic-created'));
  ws(900, 'wss://ws.bitget.com/v2/ws/public', {
    arg: { instType: category, channel: 'ticker', instId: symbol },
    action: 'snapshot',
    data: [{ instId: symbol, lastPr: '110', bidPr: '109.9', askPr: '110.1' }],
  });
  command(1100, 'amendOrder', { id: 'classic-created', amount: 0.02 });
  http(
    'POST',
    '/api/v2/mix/order/modify-plan-order',
    '',
    { orderId: 'classic-created' },
    { productType, orderId: 'classic-created', newSize: '0.02', newCallbackRatio: '5' },
  );
  command(1500, 'cancelOrders', { ids: ['classic-created'] });
  http(
    'POST',
    '/api/v2/mix/order/cancel-plan-order',
    '',
    { successList: [{ orderId: 'classic-created' }], failureList: [] },
    { symbol, productType, marginCoin: coin, orderIdList: [{ orderId: 'classic-created' }], planType: 'track_plan' },
  );
  strategy(2000, row('classic-final', '0.03', '97', '3'));
  return records;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const { values } = parseArgs({
    options: {
      out: { type: 'string', default: 'sdk-rs/harness/tapes/bitget-uta-private.jsonl' },
      usdc: { type: 'boolean', default: false },
      classic: { type: 'boolean', default: false },
    },
  });
  const records = values.classic
    ? buildBitgetClassicTrailingTape({ usdc: values.usdc })
    : buildBitgetUtaTape({ usdc: values.usdc });
  mkdirSync(dirname(values.out), { recursive: true });
  writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
  console.log(
    JSON.stringify({
      out: values.out,
      records: records.length,
      seconds: Math.max(...records.map((r) => r.t ?? 0)) / 1000,
    }),
  );
}
