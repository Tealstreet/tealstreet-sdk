// Documented Advanced Trade payloads, never captured private credentials.
// https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/orders/create-order
// https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-channels
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { COINBASE_REPLAY, COINBASE_ES256_REPLAY } from './coinbase-protocol.mjs';

export function coinbasePrivateTape({ profile = 'intx', algorithm = 'EdDSA', venue = 'coinbase' } = {}) {
  if (!['intx', 'cfm', 'spot'].includes(profile)) throw new Error('Expected profile intx, cfm, or spot');
  if (!['ES256', 'EdDSA'].includes(algorithm)) throw new Error('Expected algorithm ES256 or EdDSA');
  if (!['coinbase', 'coinbasespot'].includes(venue) || (venue === 'coinbasespot' && profile !== 'spot'))
    throw new Error('coinbasespot requires the spot profile');
  const epoch = 1765805280000;
  const rest = 'https://api.coinbase.com';
  const root = '/api/v3/brokerage';
  const publicSocket = 'wss://advanced-trade-ws.coinbase.com';
  const privateSocket = 'wss://advanced-trade-ws-user.coinbase.com';
  const main = profile === 'intx' ? 'BTC-PERP-INTX' : profile === 'cfm' ? 'BTC-CDE' : 'BTC-USD';
  const nativeSize = profile === 'cfm' ? 1 : 0.1;
  const credentials = algorithm === 'ES256' ? COINBASE_ES256_REPLAY : COINBASE_REPLAY;
  const records = [
    { t: 0, type: 'meta', private: true, epoch, venue, profile, algorithm },
    {
      t: 0,
      type: 'config',
      private: true,
      options: { ...credentials, spotOnly: venue === 'coinbasespot', testnet: false },
    },
  ];
  let clock = 0,
    commandCount = 0,
    privateSequence = 0,
    publicSequence = 0;
  const http = (method, path, body, requestBody, metadata = {}) =>
    records.push({
      t: 0,
      type: 'http',
      method,
      url: path.startsWith('https://') ? path : rest + path,
      status: 200,
      afterCommand: commandCount,
      body: JSON.stringify(body),
      ...(requestBody === undefined ? {} : { requestBody: JSON.stringify(requestBody) }),
      ...metadata,
    });
  const message = (url, channel, events) =>
    records.push({
      t: (clock += 100),
      type: 'ws',
      url,
      text: JSON.stringify({
        channel,
        sequence_num: url === privateSocket ? privateSequence++ : publicSequence++,
        events,
      }),
    });
  const user = (kind, rows) => message(privateSocket, 'user', [{ type: kind, orders: rows }]);
  const heartbeat = () => {
    message(publicSocket, 'heartbeats', [
      { current_time: new Date(epoch + clock).toISOString(), heartbeat_counter: String(publicSequence) },
    ]);
    message(privateSocket, 'heartbeats', [
      { current_time: new Date(epoch + clock).toISOString(), heartbeat_counter: privateSequence },
    ]);
  };
  const command = (data) => {
    heartbeat();
    records.push({ t: (clock += 1500), type: 'command', ...data });
    return ++commandCount;
  };
  const product = (id, type = 'SPOT', extra = {}) => ({
    product_id: id,
    product_type: type,
    base_currency_id: 'BTC',
    quote_currency_id: type === 'SPOT' ? 'USD' : 'USDC',
    status: 'TRADING',
    base_increment: type === 'FUTURE' && profile === 'cfm' ? '1' : '0.001',
    price_increment: '0.01',
    base_min_size: type === 'FUTURE' && profile === 'cfm' ? '1' : '0.001',
    base_max_size: '100',
    price: '110',
    volume_24h: '20',
    price_percentage_change_24h: '2',
    ...extra,
  });
  const future = product(main, 'FUTURE', {
    quote_currency_id: profile === 'intx' ? 'USDC' : 'USD',
    future_product_details: {
      contract_expiry_type: profile === 'intx' ? 'PERPETUAL' : 'EXPIRING',
      contract_root_unit: 'BTC',
      contract_size: profile === 'cfm' ? '0.01' : '1',
      perpetual_details: { open_interest: '30', funding_time: '2025-12-15T12:00:00Z' },
    },
  });
  const spot = product('BTC-USD');
  const productReads = (query, products) => {
    for (const suffix of [query, `${query}${query ? '&' : '?'}limit=1000`])
      http('GET', root + '/products' + suffix, {
        products,
        num_products: products.length,
        pagination: { has_next: false },
      });
  };
  if (profile === 'spot') productReads('', [spot]);
  else if (profile === 'cfm') {
    productReads('?product_type=FUTURE', [future]);
    productReads('', [spot]);
  } else productReads('?product_type=FUTURE&contract_expiry_type=PERPETUAL', [future]);
  if (profile === 'intx')
    http('GET', 'https://api.international.coinbase.com/api/v1/instruments/', [
      {
        instrument_id: '1',
        symbol: 'BTC-PERP',
        type: 'PERP',
        trading_state: 'TRADING',
        base_asset_name: 'BTC',
        quote_asset_name: 'USDC',
        base_increment: '0.001',
        quote_increment: '0.01',
        min_quantity: '0.001',
        min_notional_value: '1',
        qty_24hr: '20',
        notional_24hr: '2200',
        open_interest: '30',
        funding_interval: '3600000000',
        quote: {
          best_bid_price: '109',
          best_ask_price: '111',
          trade_price: '110',
          mark_price: '110',
          index_price: '109.9',
          predicted_funding: '0.0001',
        },
      },
    ]);
  const portfolio = {
    uuid: 'replay-portfolio',
    name: 'Replay',
    type: profile === 'intx' ? 'INTX' : 'DEFAULT',
    deleted: false,
  };
  http('GET', root + '/portfolios', { portfolios: [portfolio] });
  const breakdown = {
    breakdown: {
      portfolio,
      spot_positions: [
        { asset: 'BTC', total_balance_crypto: 1.5, available_to_trade_crypto: 1, total_balance_fiat: 165 },
        { asset: 'USD', total_balance_crypto: 1000, available_to_trade_crypto: 900, total_balance_fiat: 1000 },
      ],
      portfolio_balances: { total_balance: { value: '1165', currency: 'USD' } },
    },
  };
  http('GET', root + '/portfolios/replay-portfolio', breakdown);
  const cfmBalance = {
    balance_summary: {
      total_usd_balance: { value: '1000', currency: 'USD' },
      available_margin: { value: '800', currency: 'USD' },
      unrealized_pnl: { value: '1.5', currency: 'USD' },
    },
  };
  if (profile === 'cfm') {
    http('GET', root + '/cfm/balance_summary', cfmBalance);
    http('GET', root + '/cfm/positions', {
      positions: [
        {
          product_id: main,
          side: 'LONG',
          number_of_contracts: '10',
          avg_entry_price: '100',
          current_price: '110',
          unrealized_pnl: '1',
          daily_realized_pnl: '0',
        },
      ],
    });
  } else if (profile === 'spot' && venue === 'coinbase') {
    http(
      'GET',
      root + '/cfm/balance_summary',
      { error: 'PERMISSION_DENIED', message: 'CFM account is not enabled' },
      undefined,
      { status: 403 },
    );
  } else if (profile === 'intx') {
    http('GET', root + '/intx/portfolio/replay-portfolio', {
      summary: { total_balance: { value: '1000', currency: 'USDC' }, buying_power: { value: '800', currency: 'USDC' } },
    });
    http('GET', root + '/intx/positions/replay-portfolio', {
      positions: [
        {
          symbol: main,
          position_side: 'POSITION_SIDE_LONG',
          net_size: '2',
          entry_vwap: { value: '100' },
          mark_price: { value: '110' },
          liquidation_price: { value: '50' },
          leverage: '10',
        },
      ],
    });
  }
  const order = (id, extra = {}) => ({
    order_id: id,
    client_order_id: `resting-${id}`,
    product_id: main,
    status: 'OPEN',
    order_side: 'BUY',
    side: 'BUY',
    order_type: 'LIMIT',
    cumulative_quantity: '0',
    filled_size: '0',
    leaves_quantity: String(nativeSize),
    limit_price: '100',
    stop_price: '',
    avg_price: '0',
    average_filled_price: '0',
    order_configuration: { limit_limit_gtc: { base_size: String(nativeSize), limit_price: '100', post_only: false } },
    created_time: new Date(epoch - 60000).toISOString(),
    ...extra,
  });
  const wsOrder = (row) => {
    const { order_configuration, filled_size, average_filled_price, side, ...wire } = row;
    return {
      ...wire,
      order_type: { LIMIT: 'Limit', MARKET: 'Market', STOP_LIMIT: 'Stop Limit' }[wire.order_type] ?? wire.order_type,
    };
  };
  let active = [
    order('11'),
    order('12'),
    order('13'),
    order('21', {
      leaves_quantity: '3',
      order_configuration: { limit_limit_gtc: { base_size: '3', limit_price: '100', post_only: false } },
    }),
  ];
  const openStage = (afterCommand = 0) => {
    const kinds = profile === 'cfm' ? ['cfm', 'spot'] : [profile];
    for (const kind of kinds)
      for (const status of ['PENDING', 'OPEN', 'QUEUED', 'CANCEL_QUEUED', 'EDIT_QUEUED']) {
        const productType = kind === 'spot' ? 'SPOT' : 'FUTURE';
        const expiry =
          kind === 'cfm' ? '&contract_expiry_type=EXPIRING' : kind === 'intx' ? '&contract_expiry_type=PERPETUAL' : '';
        const path = `${root}/orders/historical/batch?product_type=${productType}${expiry}&order_status=${status}&limit=100`;
        const rows =
          status === 'OPEN'
            ? active.filter((r) => (kind === 'spot' ? r.product_id === 'BTC-USD' : r.product_id !== 'BTC-USD'))
            : [];
        const paged = afterCommand === 0 && rows.length > 2;
        http(
          'GET',
          path,
          { orders: paged ? rows.slice(0, 2) : rows, has_next: paged, cursor: paged ? 'open-page-2' : '' },
          undefined,
          { afterCommand },
        );
        if (paged)
          http('GET', path + '&cursor=open-page-2', { orders: rows.slice(2), has_next: false, cursor: '' }, undefined, {
            afterCommand,
          });
      }
  };
  openStage();
  if (profile === 'spot')
    for (const row of active) http('GET', `${root}/orders/historical/${row.order_id}`, { order: row });
  message(privateSocket, 'subscriptions', [{ subscriptions: { user: [], heartbeats: [] } }]);
  heartbeat();
  user('snapshot', active.map(wsOrder));
  for (const [quantity, remaining, average, status] of [
    ['1', '2', '100', 'OPEN'],
    ['3', '0', '102', 'FILLED'],
    ['3', '0', '102', 'FILLED'],
  ])
    user('update', [
      wsOrder(order('21', { status, cumulative_quantity: quantity, leaves_quantity: remaining, avg_price: average })),
    ]);
  active = active.filter((r) => r.order_id !== '21');
  const createBody = (client, config, side = 'BUY') => ({
    client_order_id: client,
    product_id: main,
    side,
    leverage: '10',
    order_configuration: config,
  });
  const create = (id, body) =>
    http('POST', root + '/orders', { success: true, success_response: { order_id: id } }, body);
  const cancel = (ids, results) => http('POST', root + '/orders/batch_cancel', { results }, { order_ids: ids });
  const place = command({
    op: 'placeOrders',
    orders: [
      {
        symbol: main,
        type: 'limit',
        side: 'buy',
        amount: nativeSize,
        price: 99,
        clientOrderId: 'CoinbaseCaller-30',
        timeInForce: 'PostOnly',
      },
    ],
  });
  create(
    '30',
    createBody('CoinbaseCaller-30', {
      limit_limit_gtc: { base_size: String(nativeSize), limit_price: '99', post_only: true },
    }),
  );
  const thirty = order('30', {
    client_order_id: 'CoinbaseCaller-30',
    limit_price: '99',
    order_configuration: { limit_limit_gtc: { base_size: String(nativeSize), limit_price: '99', post_only: true } },
  });
  active.push(thirty);
  openStage(place);
  user('update', [wsOrder(thirty)]);
  const amend = command({ op: 'amendOrder', id: '11', price: 101 });
  if (profile === 'intx') {
    cancel(['11'], [{ order_id: '11', success: true }]);
    http('GET', root + '/orders/historical/11', {
      order: { order_id: '11', product_id: main, status: 'CANCELLED', filled_size: '0' },
    });
    create(
      '31',
      createBody('tealstreet-replacement', {
        limit_limit_gtc: { base_size: String(nativeSize), limit_price: '101', post_only: false },
      }),
    );
    active = active.filter((r) => r.order_id !== '11');
    active.push(
      order('31', {
        client_order_id: 'tealstreet-replacement',
        limit_price: '101',
        order_configuration: {
          limit_limit_gtc: { base_size: String(nativeSize), limit_price: '101', post_only: false },
        },
      }),
    );
  } else if (profile === 'spot') {
    http(
      'POST',
      root + '/orders/edit',
      { success: true, errors: [] },
      { order_id: '11', price: '101', size: String(nativeSize) },
    );
    active = active.map((r) =>
      r.order_id === '11'
        ? order('11', {
            limit_price: '101',
            order_configuration: {
              limit_limit_gtc: { base_size: String(nativeSize), limit_price: '101', post_only: false },
            },
          })
        : r,
    );
  }
  openStage(amend);
  user('snapshot', active.map(wsOrder));
  const marketCommand = command({
    op: 'placeOrders',
    orders: [{ symbol: main, type: 'market', side: 'buy', amount: nativeSize, clientOrderId: 'CoinbaseCaller-40' }],
  });
  create('40', createBody('CoinbaseCaller-40', { market_market_ioc: { base_size: String(nativeSize) } }));
  user('update', [
    wsOrder(
      order('40', {
        client_order_id: 'CoinbaseCaller-40',
        order_type: 'MARKET',
        limit_price: '',
        status: 'FILLED',
        cumulative_quantity: String(nativeSize),
        leaves_quantity: '0',
        avg_price: '110',
      }),
    ),
  ]);
  openStage(marketCommand);
  const stopCommand = command({
    op: 'placeOrders',
    orders: [{ symbol: main, type: 'stop_market', side: 'sell', amount: nativeSize, price: 90 }],
  });
  create(
    '50',
    createBody(
      'tealstreet-stop',
      {
        stop_limit_stop_limit_gtc: {
          base_size: String(nativeSize),
          limit_price: '89.1',
          stop_price: '90',
          stop_direction: 'STOP_DIRECTION_STOP_DOWN',
        },
      },
      'SELL',
    ),
  );
  const fifty = order('50', {
    client_order_id: 'tealstreet-stop',
    order_type: 'STOP_LIMIT',
    order_side: 'SELL',
    side: 'SELL',
    stop_price: '90',
    limit_price: '89.1',
    order_configuration: {
      stop_limit_stop_limit_gtc: {
        base_size: String(nativeSize),
        limit_price: '89.1',
        stop_price: '90',
        stop_direction: 'STOP_DIRECTION_STOP_DOWN',
      },
    },
  });
  active.push(fifty);
  openStage(stopCommand);
  user('update', [wsOrder(fifty)]);
  const partial = command({ op: 'cancelOrders', ids: ['12', '13'] });
  cancel(
    ['12', '13'],
    [
      { order_id: '12', success: true },
      { order_id: '13', success: false, failure_reason: 'UNKNOWN_CANCEL_ORDER' },
    ],
  );
  user('update', [wsOrder(order('12', { status: 'CANCELLED' }))]);
  active = active.filter((r) => r.order_id !== '12');
  openStage(partial);
  const cancelThirty = command({ op: 'cancelOrders', ids: ['30'] });
  cancel(['30'], [{ order_id: '30', success: true }]);
  user('update', [wsOrder({ ...thirty, status: 'CANCELLED' })]);
  active = active.filter((r) => r.order_id !== '30');
  openStage(cancelThirty);
  command({ op: 'setLeverage', symbol: main, leverage: 10 });
  command({ op: 'setMarginType', symbol: main, isolated: false });
  command({ op: 'setPositionMode', hedged: false });
  command({ op: 'fetchLeverage', symbol: main });
  const from = epoch - 1000,
    to = epoch + 60000;
  command({ op: 'fetchFills', symbol: main, from, to, limit: 2 });
  const fills = [
    {
      entry_id: 'entry-one',
      trade_id: 'fill-1',
      order_id: '21',
      product_id: main,
      trade_type: 'FILL',
      trade_time: new Date(epoch + 1).toISOString(),
      price: '100',
      size: '1',
      size_in_quote: false,
      commission: '0.1',
      liquidity_indicator: 'MAKER',
      side: 'BUY',
    },
    {
      entry_id: 'entry-two',
      trade_id: 'fill-2',
      order_id: '21',
      product_id: main,
      trade_type: 'FILL',
      trade_time: new Date(epoch + 2).toISOString(),
      price: '103',
      size: '2',
      size_in_quote: false,
      commission: '0.206',
      liquidity_indicator: 'TAKER',
      side: 'BUY',
    },
  ];
  http(
    'GET',
    `${root}/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_ids=${main}&product_types=${profile === 'spot' ? 'SPOT' : 'FUTURE'}&start_sequence_timestamp=${encodeURIComponent(new Date(from - 1).toISOString())}&end_sequence_timestamp=${encodeURIComponent(new Date(to + 1).toISOString())}`,
    { fills: [fills[1]], cursor: 'fill-page-2', has_next: true, proof_token_required: false },
  );
  records.push({
    ...records.at(-1),
    url: records.at(-1).url + '&cursor=fill-page-2',
    body: JSON.stringify({ fills: [fills[1], fills[0]], cursor: '', has_next: false, proof_token_required: false }),
  });
  command({ op: 'fetchOrderHistory', symbol: main, from, to, limit: 2 });
  const terminal = order('80', {
    status: 'FILLED',
    filled_size: '1',
    cumulative_quantity: '1',
    leaves_quantity: '0',
    avg_price: '100',
    average_filled_price: '100',
    created_time: new Date(epoch).toISOString(),
    order_configuration: { limit_limit_gtc: { base_size: '1', limit_price: '100', post_only: false } },
  });
  const type = profile === 'spot' ? 'SPOT' : 'FUTURE',
    expiry =
      profile === 'cfm'
        ? '&contract_expiry_type=EXPIRING'
        : profile === 'intx'
          ? '&contract_expiry_type=PERPETUAL'
          : '';
  http(
    'GET',
    `${root}/orders/historical/batch?limit=2&order_status=FILLED&order_status=CANCELLED&order_status=EXPIRED&order_status=FAILED&product_ids=${main}&product_type=${type}${expiry}&start_date=${encodeURIComponent(new Date(from).toISOString())}&end_date=${encodeURIComponent(new Date(to + 1).toISOString())}`,
    { orders: [terminal], has_next: true, cursor: 'history-page-2' },
  );
  const cancelled = order('81', {
    status: 'CANCELLED',
    created_time: new Date(epoch - 100).toISOString(),
    limit_price: '101',
    order_configuration: { limit_limit_gtc: { base_size: String(nativeSize), limit_price: '101', post_only: false } },
  });
  records.push({
    ...records.at(-1),
    url: records.at(-1).url + '&cursor=history-page-2',
    body: JSON.stringify({ orders: [cancelled], has_next: false, cursor: '' }),
  });
  command({ op: 'fetchPublicTrades', symbol: main, limit: 2 });
  http('GET', `${root}/market/products/${main}/ticker?limit=2`, {
    trades: [
      {
        trade_id: 'public-1',
        product_id: main,
        price: '110',
        size: profile === 'cfm' ? '1' : '0.1',
        side: 'BUY',
        time: new Date(epoch + 10).toISOString(),
      },
      {
        trade_id: 'public-2',
        product_id: main,
        price: '109',
        size: profile === 'cfm' ? '2' : '0.2',
        side: 'SELL',
        time: new Date(epoch + 20).toISOString(),
      },
    ],
    best_bid: '109',
    best_ask: '111',
  });
  command({ op: 'fetchIncome', symbol: main, from, to, limit: 2 });
  const candleCommand = command({
    op: 'fetchCandles',
    symbol: main,
    interval: '1m',
    from: epoch - 120000,
    to: epoch,
    limit: 2,
  });
  const candles = {
    candles: [
      { start: String(epoch / 1000 - 60), open: '100', high: '110', low: '99', close: '105', volume: '5' },
      { start: String(epoch / 1000 - 120), open: '99', high: '101', low: '98', close: '100', volume: '3' },
    ],
  };
  for (const route of [`/products/${main}/candles`, `/market/products/${main}/candles`])
    http(
      'GET',
      `${root}${route}?granularity=ONE_MINUTE&start=${epoch / 1000 - 120}&end=${epoch / 1000}&limit=2`,
      candles,
      undefined,
      { afterCommand: candleCommand },
    );
  // TS sends the limit only in its range calculation.
  http(
    'GET',
    `${root}/products/${main}/candles?granularity=ONE_MINUTE&start=${epoch / 1000 - 120}&end=${epoch / 1000}`,
    candles,
    undefined,
    { afterCommand: candleCommand },
  );
  user('snapshot', active.map(wsOrder));
  heartbeat();
  records.sort((a, b) => a.t - b.t);
  return { records, commands: commandCount, seconds: clock / 1000, profile, algorithm, venue };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({
    options: {
      out: { type: 'string' },
      profile: { type: 'string', default: 'intx' },
      algorithm: { type: 'string', default: 'EdDSA' },
      venue: { type: 'string', default: 'coinbase' },
    },
  });
  const tape = coinbasePrivateTape(values);
  const out =
    values.out ??
    (values.venue === 'coinbase' && values.profile === 'intx' && values.algorithm === 'EdDSA'
      ? 'sdk-rs/harness/tapes/coinbase-private.jsonl'
      : `sdk-rs/harness/tapes/${values.venue}-${values.profile}-${values.algorithm.toLowerCase()}-private.jsonl`);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, tape.records.map((r) => JSON.stringify(r)).join('\n') + '\n');
  console.log(
    JSON.stringify({
      out,
      records: tape.records.length,
      commands: tape.commands,
      seconds: tape.seconds,
      profile: tape.profile,
      algorithm: tape.algorithm,
      venue: tape.venue,
    }),
  );
}
