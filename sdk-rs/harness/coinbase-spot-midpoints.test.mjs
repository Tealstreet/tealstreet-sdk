import assert from 'node:assert/strict';
import test from 'node:test';
import { compareCoinbaseSpotMidpoints } from './coinbase-spot-midpoints.mjs';

const clone = (value) => structuredClone(value);
const native = (product_id, price, volume_24h, approximate_quote_24h_volume) => ({
  product_id,
  price,
  volume_24h,
  approximate_quote_24h_volume,
  product_type: 'SPOT',
  future_product_details: null,
  status: 'online',
  is_disabled: false,
  trading_disabled: false,
  base_increment: '1',
  price_increment: '0.00001',
  base_min_size: '1',
  base_max_size: '100000000',
  base_currency_id: product_id.split('-')[0],
  quote_currency_id: product_id.split('-')[1],
});
const catalog = (products, requestOrdinal = 0, t = 10) => ({
  type: 'http',
  method: 'GET',
  status: 200,
  t,
  requestOrdinal,
  url: 'https://api.coinbase.com/api/v3/brokerage/market/products?limit=1000',
  body: JSON.stringify({
    products,
    num_products: products.length,
    pagination: { has_next: false, has_prev: false, next_cursor: '', prev_cursor: '' },
  }),
});
function fixture() {
  // Handwritten native rows and expected model values, not a model-derived oracle.
  const rows = [
    native('DEEP-USD', '0.01875', '2625844', '49234.58'),
    native('DEEP-USDC', '0.01875', '2625844', '49234.58'),
    native('WET-USD', '0.05', '68392.9', '3419.65'),
    native('WET-USDC', '0.05', '68392.9', '3419.65'),
  ];
  const markets = rows.map((r) => ({
    id: r.product_id,
    symbol: r.product_id,
    base: r.base_currency_id,
    quote: r.quote_currency_id,
    isSpot: true,
    isInverse: false,
    isExpiring: false,
    contractSize: 1,
    active: true,
    precision: { price: 0.00001, amount: 1 },
  }));
  const tickers = [
    { symbol: 'DEEP-USD', last: 0.01875, volume: 2625844, quoteVolume: 49234.57, bid: 0.01875 },
    { symbol: 'DEEP-USDC', last: 0.01875, volume: 2625844, quoteVolume: 49234.57, bid: 0.01875 },
    { symbol: 'WET-USD', last: 0.05, volume: 68392.9, quoteVolume: 3419.64, bid: 0.05 },
    { symbol: 'WET-USDC', last: 0.05, volume: 68392.9, quoteVolume: 3419.64, bid: 0.05 },
  ];
  const ts = { markets, tickers, commands: [] };
  const rust = clone(ts);
  rust.tickers[0].quoteVolume = rust.tickers[1].quoteVolume = 49234.58;
  rust.tickers[2].quoteVolume = rust.tickers[3].quoteVolume = 3419.65;
  return { venue: 'coinbase', records: [{ type: 'meta', t: 0, private: false }, catalog(rows)], ts, rust };
}
function changeRows(f, fn) {
  const body = JSON.parse(f.records.at(-1).body);
  fn(body.products, body);
  f.records.at(-1).body = JSON.stringify(body);
}
function freeze(value) {
  if (value && typeof value === 'object') {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

test('proves all four independent native midpoint rows without mutating either model or tape', () => {
  const f = fixture(),
    before = JSON.stringify(f);
  const result = compareCoinbaseSpotMidpoints(freeze(f));
  assert.equal(result.equivalent, true, result.reason);
  assert.deepEqual(
    result.exceptions.map((r) => r.symbol),
    ['DEEP-USD', 'DEEP-USDC', 'WET-USD', 'WET-USDC'],
  );
  assert.deepEqual(result.exceptions[0], {
    symbol: 'DEEP-USD',
    ts: 49234.57,
    rust: 49234.58,
    native: '49234.58',
    notional: { numerator: '4923457500', denominator: '100000' },
    requestOrdinal: 0,
    timestamp: 10,
    candidateRequestOrdinals: [0],
  });
  assert.equal(JSON.stringify(f), before);
});
test('accepts the same proof for saved spot alias and either model choosing either cent neighbor', () => {
  const f = fixture();
  f.venue = 'coinbasespot';
  f.records[0].venue = 'coinbase';
  [f.ts, f.rust] = [f.rust, f.ts];
  changeRows(f, (rows) => {
    rows[0].approximate_quote_24h_volume = '49234.57';
  });
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, true);
});
test('accepts public heartbeat and book records with one unambiguous native sequence', () => {
  const f = fixture();
  f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
  f.records.push({
    type: 'ws',
    socket: 0,
    url: 'wss://advanced-trade-ws.coinbase.com',
    text: '{"channel":"heartbeats","sequence_num":0}',
  });
  f.records.push({
    type: 'ws',
    socket: 0,
    url: 'wss://advanced-trade-ws.coinbase.com',
    text: '{"channel":"l2_data","sequence_num":1}',
  });
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, true);
});
for (const binary of [
  Buffer.from('{"channel":"ticker","sequence_num":0}').toString('base64'),
  Buffer.from('{"channel":"user","sequence_num":0}').toString('base64'),
  undefined,
])
  test(`rejects a text heartbeat with an independent binary payload ${binary}`, () => {
    const f = fixture();
    f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
    f.records.push({
      type: 'ws',
      socket: 0,
      url: 'wss://advanced-trade-ws.coinbase.com',
      text: '{"channel":"heartbeats","sequence_num":0}',
      binary,
    });
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  });
test('requires the latest validated catalog to match the observed output fields', () => {
  const f = fixture();
  const rows = JSON.parse(f.records[1].body).products;
  f.records.push(catalog(rows, 1, 20));
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, true);
  changeRows(f, (r) => {
    r[0].volume_24h = '2625840';
  });
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
for (const issue of ['rounded-price-collision', 'reused-identity', 'missing-corroboration'])
  test(`rejects an ambiguous earlier matching catalog: ${issue}`, () => {
    const f = fixture();
    const latest = JSON.parse(f.records[1].body).products;
    changeRows(f, (rows) => {
      if (issue === 'rounded-price-collision') rows[0].price = '0.018750000000000001';
      if (issue === 'reused-identity') rows[0].base_currency_id = 'DIFFERENT';
      if (issue === 'missing-corroboration') delete rows[0].approximate_quote_24h_volume;
    });
    f.records.push(catalog(latest, 1, 20));
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  });
test('permits varying catalogs only when every numerically matching candidate proves the same exact notional', () => {
  const f = fixture();
  const latest = JSON.parse(f.records[1].body).products;
  changeRows(f, (rows) => {
    rows[0].volume_24h = '2625843'; // Cannot be the source of the observed base volume.
    rows[1].price = '0.01875000';
    rows[1].volume_24h = '2625844.00'; // Equivalent rationals need not share decimal encoding.
  });
  f.records.push(catalog(latest, 1, 20));
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, true);
});
for (const key of ['products', '\\u0070roducts'])
  test(`rejects duplicate raw catalog members decoded as ${key}`, () => {
    const f = fixture();
    const original = JSON.parse(f.records[1].body);
    const first = clone(original.products);
    first[0].price = '0.018750000000000001';
    f.records[1].body = '{"products":' + JSON.stringify(first) + ',"' + key + '":' +
      JSON.stringify(original.products) + ',"num_products":4,"pagination":' + JSON.stringify(original.pagination) + '}';
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  });
test('failed later catalog reads retain the previous validated candidate', () => {
  const f = fixture();
  f.records.push({ ...f.records[1], requestOrdinal: 1, t: 20, status: 503, body: 'unavailable' });
  const result = compareCoinbaseSpotMidpoints(f);
  assert.equal(result.equivalent, true);
  assert.equal(result.exceptions[0].requestOrdinal, 0);
  assert.equal(result.exceptions[0].timestamp, 10);
});

const rejected = [
  [
    'other venue',
    (f) => {
      f.venue = 'binance';
    },
  ],
  [
    'missing explicit public metadata',
    (f) => {
      delete f.records[0].private;
    },
  ],
  [
    'private metadata',
    (f) => {
      f.records[0].private = true;
    },
  ],
  [
    'second contradictory metadata',
    (f) => {
      f.records.push({ type: 'meta', private: true });
    },
  ],
  [
    'CFM profile',
    (f) => {
      f.records[0].profile = 'cfm';
    },
  ],
  [
    'INTX profile',
    (f) => {
      f.records[0].profile = 'intx';
    },
  ],
  [
    'implementation-specific catalog',
    (f) => {
      f.records[1].impl = 'rust';
    },
  ],
  [
    'private key despite public metadata',
    (f) => {
      f.records.push({ type: 'config', options: { key: 'x' } });
    },
  ],
  [
    'contradictory public-only option',
    (f) => {
      f.records.push({ type: 'config', options: { publicDataOnly: false } });
    },
  ],
  [
    'contradictory spot option',
    (f) => {
      f.records.push({ type: 'config', options: { spotOnly: false } });
    },
  ],
  [
    'private HTTP despite public metadata',
    (f) => {
      f.records.push({ ...f.records[1], url: 'https://api.coinbase.com/api/v3/brokerage/accounts' });
    },
  ],
  [
    'INTX HTTP source',
    (f) => {
      f.records[1].url = 'https://api.coinbase.com/api/v3/brokerage/intx/instruments';
    },
  ],
  [
    'private user socket',
    (f) => {
      f.records.push({ type: 'ws', url: 'wss://advanced-trade-ws-user.coinbase.com', text: '{"channel":"user"}' });
    },
  ],
  [
    'future ticker-revising socket',
    (f) => {
      f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
      f.records.push({
        type: 'ws',
        socket: 0,
        url: 'wss://advanced-trade-ws.coinbase.com',
        text: '{"channel":"ticker","sequence_num":0}',
      });
    },
  ],
  [
    'unsupported binary record',
    (f) => {
      f.records.push({ type: 'ws_binary' });
    },
  ],
  [
    'taped command',
    (f) => {
      f.records.push({ type: 'command', op: 'placeOrders' });
    },
  ],
  [
    'missing catalog ordinal',
    (f) => {
      delete f.records[1].requestOrdinal;
    },
  ],
  [
    'repeated catalog ordinal',
    (f) => {
      f.records.push({ ...f.records[1], t: 20 });
    },
  ],
  [
    'out-of-order catalog timestamp',
    (f) => {
      f.records.push({ ...f.records[1], requestOrdinal: 1, t: 9 });
    },
  ],
  [
    'cursor request',
    (f) => {
      f.records[1].url += '&cursor=next';
    },
  ],
  [
    'partial pagination',
    (f) => {
      changeRows(f, (_, b) => {
        b.pagination.has_next = true;
        b.pagination.next_cursor = 'next';
      });
    },
  ],
  [
    'missing pagination evidence',
    (f) => {
      changeRows(f, (_, b) => {
        delete b.pagination;
      });
    },
  ],
  [
    'wrong catalog count',
    (f) => {
      changeRows(f, (_, b) => {
        b.num_products = 99;
      });
    },
  ],
  [
    'duplicate native row',
    (f) => {
      changeRows(f, (r, b) => {
        r.push(clone(r[0]));
        b.num_products++;
      });
    },
  ],
  [
    'missing alias native row',
    (f) => {
      changeRows(f, (r, b) => {
        r.splice(1, 1);
        b.num_products--;
        r[0].alias_to = ['DEEP-USDC'];
      });
    },
  ],
  [
    'duplicate output ticker',
    (f) => {
      f.rust.tickers.push(clone(f.rust.tickers[0]));
    },
  ],
  [
    'duplicate output market',
    (f) => {
      f.ts.markets.push(clone(f.ts.markets[0]));
    },
  ],
  [
    'missing output ticker',
    (f) => {
      f.rust.tickers.pop();
    },
  ],
  [
    'changed ticker identity',
    (f) => {
      f.rust.tickers[0].symbol = 'DEEP-EUR';
    },
  ],
  [
    'wrong base identity',
    (f) => {
      f.rust.markets[0].base = 'BTC';
    },
  ],
  [
    'wrong quote identity',
    (f) => {
      f.rust.markets[0].quote = 'USDC';
    },
  ],
  [
    'wrong market symbol',
    (f) => {
      f.rust.markets[0].symbol = 'DEEP-USDC';
    },
  ],
  [
    'wrong contract size',
    (f) => {
      f.rust.markets[0].contractSize = 10;
    },
  ],
  [
    'nonspot normalized market',
    (f) => {
      f.rust.markets[0].isSpot = false;
    },
  ],
  [
    'future native row',
    (f) => {
      changeRows(f, (r) => {
        r[0].product_type = 'FUTURE';
      });
    },
  ],
  [
    'future details on SPOT',
    (f) => {
      changeRows(f, (r) => {
        r[0].future_product_details = { contract_size: '1' };
      });
    },
  ],
  [
    'different other ticker field',
    (f) => {
      f.rust.tickers[0].bid = 1;
    },
  ],
  [
    'different other market field',
    (f) => {
      f.rust.markets[0].precision.amount = 2;
    },
  ],
  [
    'changed price',
    (f) => {
      f.rust.tickers[0].last = 0.01876;
    },
  ],
  [
    'changed base volume',
    (f) => {
      f.rust.tickers[0].volume = 2625843;
    },
  ],
  [
    'same wrong price in both models',
    (f) => {
      f.ts.tickers[0].last = f.rust.tickers[0].last = 1;
    },
  ],
  [
    'same wrong base volume in both models',
    (f) => {
      f.ts.tickers[0].volume = f.rust.tickers[0].volume = 1;
    },
  ],
  [
    'same wrong quote volume in both models',
    (f) => {
      f.ts.tickers[0].quoteVolume = f.rust.tickers[0].quoteVolume = 49234.59;
    },
  ],
  [
    'more than half cent formula error',
    (f) => {
      f.rust.tickers[0].quoteVolume = 49234.59;
    },
  ],
  [
    'non-cent output',
    (f) => {
      f.rust.tickers[0].quoteVolume = 49234.575;
    },
  ],
  [
    'non-midpoint adjacent cents',
    (f) => {
      changeRows(f, (r) => {
        r[0].price = '0.018750001';
      });
      f.ts.tickers[0].last = f.rust.tickers[0].last = 0.018750001;
    },
  ],
  [
    'missing native corroboration',
    (f) => {
      changeRows(f, (r) => {
        delete r[0].approximate_quote_24h_volume;
      });
    },
  ],
  [
    'contradictory native corroboration',
    (f) => {
      changeRows(f, (r) => {
        r[0].approximate_quote_24h_volume = '49234.59';
      });
    },
  ],
  [
    'non-cent native corroboration',
    (f) => {
      changeRows(f, (r) => {
        r[0].approximate_quote_24h_volume = '49234.575';
      });
    },
  ],
  [
    'nonfinite output',
    (f) => {
      f.rust.tickers[0].quoteVolume = Infinity;
    },
  ],
  [
    'NaN output',
    (f) => {
      f.rust.tickers[0].quoteVolume = NaN;
    },
  ],
  [
    'negative output',
    (f) => {
      f.rust.tickers[0].quoteVolume = -49234.58;
    },
  ],
  [
    'malformed raw price',
    (f) => {
      changeRows(f, (r) => {
        r[0].price = 'NaN';
      });
    },
  ],
  [
    'unbounded exponent',
    (f) => {
      changeRows(f, (r) => {
        r[0].price = '1e10000000';
      });
    },
  ],
  [
    'excessive decimal digits',
    (f) => {
      changeRows(f, (r) => {
        r[0].price = '1'.repeat(10000);
      });
    },
  ],
  [
    'negative raw volume',
    (f) => {
      changeRows(f, (r) => {
        r[0].volume_24h = '-2625844';
      });
    },
  ],
  [
    'bad raw JSON',
    (f) => {
      f.records[1].body = '{';
    },
  ],
];
rejected.push(
  [
    'other family metadata',
    (f) => {
      f.records[0].venue = 'binance';
    },
  ],
  [
    'invalid native precision',
    (f) => {
      changeRows(f, (r) => {
        r[0].base_increment = '0';
      });
    },
  ],
  [
    'invalid native limits',
    (f) => {
      changeRows(f, (r) => {
        r[0].base_max_size = '0';
      });
    },
  ],
  [
    'invalid native flags',
    (f) => {
      changeRows(f, (r) => {
        r[0].trading_disabled = 'false';
      });
    },
  ],
  [
    'missing native product status',
    (f) => {
      changeRows(f, (r) => {
        delete r[0].status;
      });
    },
  ],
  [
    'invalid later success cannot reuse earlier proof',
    (f) => {
      f.records.push({ ...f.records[1], requestOrdinal: 1, t: 20 });
      changeRows(f, (r) => {
        delete r[0].base_increment;
      });
    },
  ],
  [
    'socket gap makes generation provenance ambiguous',
    (f) => {
      f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
      f.records.push({
        type: 'ws',
        socket: 0,
        url: 'wss://advanced-trade-ws.coinbase.com',
        text: '{"channel":"heartbeats","sequence_num":3}',
      });
    },
  ],
  [
    'multiple open generations',
    (f) => {
      f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
      f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 1 });
    },
  ],
  [
    'wrong socket ownership',
    (f) => {
      f.records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
      f.records.push({
        type: 'ws',
        socket: 1,
        url: 'wss://advanced-trade-ws.coinbase.com',
        text: '{"channel":"heartbeats","sequence_num":0}',
      });
    },
  ],
);
for (const [label, mutate] of rejected)
  test(`rejects ${label}`, () => {
    const f = fixture();
    mutate(f);
    const result = compareCoinbaseSpotMidpoints(f);
    assert.equal(result.equivalent, false, label);
    assert.deepEqual(result.exceptions, []);
    assert.equal(typeof result.reason, 'string');
  });
for (const field of ['availableAt', 'effectiveAt', 'availableAfter', 'afterCommand', 'requestBody', 'tsRequestBody'])
  test(`rejects unmodeled response selector ${field}`, () => {
    const f = fixture();
    f.records[1][field] = field.endsWith('Body') ? '{}' : 0;
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  });
test('rejects an unreachable later catalog instead of treating it as committed corroboration', () => {
  const f = fixture();
  f.records.push({ ...f.records[1], t: 20, requestOrdinal: 1, afterCommand: 1 });
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
test('rejects future unknown HTTP selector fields and explicit implementation properties', () => {
  for (const fields of [{ unknownRequestSelector: 'future' }, { impl: undefined }]) {
    const f = fixture();
    Object.assign(f.records[1], fields);
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  }
});
for (const section of ['balance', 'orders', 'fills', 'fees'])
  test(`account ${section} remains excluded despite a valid public midpoint`, () => {
    const f = fixture();
    f.rust.account = { [section]: [{ amount: 999 }] };
    assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
  });
test('changed command result cannot be accepted through the public midpoint comparison', () => {
  const f = fixture();
  f.rust.commands = [{ ok: { price: 99 } }];
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
test('sub1000 midpoint never permits adjacent cents', () => {
  const f = fixture();
  changeRows(f, (r) => {
    r[0].price = '0.0001';
    r[0].volume_24h = '100050';
    r[0].approximate_quote_24h_volume = '10.01';
  });
  for (const model of [f.ts, f.rust]) {
    model.tickers[0].last = 0.0001;
    model.tickers[0].volume = 100050;
  }
  f.ts.tickers[0].quoteVolume = 10;
  f.rust.tickers[0].quoteVolume = 10.01;
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
test('rejects magnitudes with inadequate cent resolution', () => {
  const f = fixture();
  changeRows(f, (r) => {
    r[0].price = '4294967296.005';
    r[0].volume_24h = '1';
    r[0].approximate_quote_24h_volume = '4294967296.01';
  });
  for (const model of [f.ts, f.rust]) {
    model.tickers[0].last = 4294967296.005;
    model.tickers[0].volume = 1;
  }
  f.ts.tickers[0].quoteVolume = 4294967296;
  f.rust.tickers[0].quoteVolume = 4294967296.01;
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
test('ordinary matching ticker rows do not claim a representation exception', () => {
  const f = fixture();
  f.ts = clone(f.rust);
  assert.equal(compareCoinbaseSpotMidpoints(f).equivalent, false);
});
