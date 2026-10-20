import { isDeepStrictEqual } from 'node:util';
import { parseUniqueJson } from './json-unique.mjs';

const root = 'https://api.coinbase.com';
const catalogPath = '/api/v3/brokerage/market/products';
const fail = (message) => {
  throw new Error(message);
};
const requireThat = (condition, message) => {
  if (!condition) fail(message);
};

// Bounded decimal integers, not either implementation's binary rounding policy.
function decimal(text) {
  requireThat(
    typeof text === 'string' && text.length <= 74 && /^(?:0|[1-9]\d{0,47})(?:\.\d{1,24})?$/.test(text),
    'invalid raw decimal',
  );
  const [whole, fraction = ''] = text.split('.');
  return { n: BigInt(whole + fraction), d: 10n ** BigInt(fraction.length) };
}
function unique(rows, field) {
  requireThat(Array.isArray(rows) && rows.length > 0 && rows.length <= 10000, 'invalid row collection');
  const out = new Map();
  for (const row of rows) {
    requireThat(
      row && typeof row[field] === 'string' && row[field].length > 0 && !out.has(row[field]),
      'missing or duplicate identity',
    );
    out.set(row[field], row);
  }
  return out;
}
function publicCatalog(venue, records) {
  requireThat(['coinbase', 'coinbasespot'].includes(venue), 'not Coinbase public spot');
  requireThat(Array.isArray(records) && records.length <= 100000, 'invalid tape');
  const metas = records.filter((r) => r.type === 'meta');
  requireThat(
    metas.length === 1 && metas[0] === records[0] && metas[0].private === false,
    'explicit public tape required',
  );
  const candidates = [];
  let publication,
    ordinal = -1,
    time = -1,
    sequence = -1,
    socket;
  for (const record of records) {
    requireThat(['meta', 'config', 'http', 'open', 'ws'].includes(record.type), 'unsupported tape record');
    requireThat(
      ['binary', 'impl', 'availableAt', 'effectiveAt', 'availableAfter', 'afterCommand', 'requestBody', 'tsRequestBody'].every(
        (key) => !Object.hasOwn(record, key),
      ),
      'unmodeled tape request selector',
    );
    requireThat(
      record.private !== true &&
        record.impl === undefined &&
        (record.profile === undefined || record.profile === 'spot') &&
        (record.venue === undefined || ['coinbase', 'coinbasespot'].includes(record.venue)),
      'contradictory tape evidence',
    );
    if (record.type === 'config') {
      const options = record.options ?? {};
      requireThat(
        !options.key &&
          !options.secret &&
          !options.passphrase &&
          (options.publicDataOnly === undefined || options.publicDataOnly === true) &&
          (options.spotOnly === undefined || options.spotOnly === true) &&
          (options.profile === undefined || options.profile === 'spot') &&
          (options.extra?.coinbasePublicSpot === undefined || options.extra.coinbasePublicSpot === true),
        'private or nonspot configuration',
      );
    }
    if (record.type === 'open' || record.type === 'ws') {
      const url = new URL(record.url);
      requireThat(
        url.protocol === 'wss:' &&
          url.hostname === 'advanced-trade-ws.coinbase.com' &&
          !url.username &&
          !url.password &&
          url.pathname === '/' &&
          !url.search &&
          !url.hash,
        'nonpublic socket source',
      );
      if (record.type === 'open') {
        requireThat(
          socket === undefined && Number.isSafeInteger(record.socket) && record.socket >= 0,
          'ambiguous socket generation',
        );
        socket = record.socket;
        continue;
      }
      requireThat(socket !== undefined && record.socket === socket, 'unproven socket ownership');
      const message = parseUniqueJson(record.text);
      requireThat(
        ['heartbeats', 'subscriptions', 'l2_data', 'candles', 'market_trades'].includes(message.channel),
        'nonpublic or ticker-revising socket source',
      );
      requireThat(
        Number.isSafeInteger(message.sequence_num) && message.sequence_num === sequence + 1,
        'ambiguous socket generation or sequence',
      );
      sequence = message.sequence_num;
    }
    if (record.type !== 'http') continue;
    requireThat(
      Object.keys(record).every((key) =>
        ['type', 't', 'requestOrdinal', 'method', 'url', 'status', 'headers', 'body'].includes(key),
      ),
      'unmodeled HTTP record field',
    );
    const url = new URL(record.url);
    requireThat(
      url.origin === root &&
        !url.username &&
        !url.password &&
        record.method === 'GET' &&
        (url.pathname === catalogPath ||
          /^\/api\/v3\/brokerage\/market\/products\/[^/]+\/(candles|ticker)$/.test(url.pathname)),
      'nonpublic HTTP source',
    );
    if (url.pathname !== catalogPath) continue;
    requireThat(
      Number.isSafeInteger(record.requestOrdinal) &&
        record.requestOrdinal > ordinal &&
        Number.isFinite(record.t) &&
        record.t > time,
      'ambiguous catalog publication order',
    );
    ordinal = record.requestOrdinal;
    time = record.t;
    requireThat(
      [...url.searchParams].every(([key, value]) => key === 'limit' && value === '1000') &&
        url.searchParams.getAll('limit').length === 1,
      'paginated or ambiguous catalog request',
    );
    requireThat(Number.isInteger(record.status), 'invalid HTTP status');
    if (record.status < 200 || record.status >= 300) continue;
    requireThat(
      typeof record.body === 'string' && record.body.length <= 16 * 1024 * 1024,
      'invalid catalog response size',
    );
    const body = parseUniqueJson(record.body);
    requireThat(
      body.pagination?.has_next === false &&
        body.pagination?.has_prev === false &&
        body.pagination.next_cursor === '' &&
        body.pagination.prev_cursor === '' &&
        body.num_products === body.products?.length,
      'incomplete catalog snapshot',
    );
    const snapshot = unique(body.products, 'product_id');
    for (const row of snapshot.values()) {
      requireThat(
        row.product_type === 'SPOT' &&
          row.future_product_details == null &&
          typeof row.base_currency_id === 'string' &&
          row.base_currency_id.length > 0 &&
          typeof row.quote_currency_id === 'string' &&
          row.quote_currency_id.length > 0,
        'nonspot or unidentified native product',
      );
      // A successful envelope containing an invalid product cannot publish a catalog.
      requireThat(
        typeof row.status === 'string' &&
          (row.price_percentage_change_24h === undefined || typeof row.price_percentage_change_24h === 'string') &&
          ['is_disabled', 'trading_disabled'].every((key) => row[key] === undefined || typeof row[key] === 'boolean'),
        'invalid native product flags',
      );
      const increment = Number(row.base_increment),
        priceStep = Number(row.price_increment);
      const minimum = Number(row.base_min_size),
        maximum = Number(row.base_max_size);
      for (const key of ['base_increment', 'price_increment', 'base_min_size', 'base_max_size']) decimal(row[key]);
      requireThat(
        [increment, priceStep, minimum, maximum].every(Number.isFinite) &&
          increment > 0 &&
          priceStep > 0 &&
          minimum >= 0 &&
          maximum >= minimum,
        'invalid native product precision or limits',
      );
    }
    publication = { snapshot, ordinal, time };
    candidates.push(publication);
  }
  requireThat(publication, 'missing validated catalog');
  return { ...publication, candidates };
}

function cents(value) {
  // Below this ceiling a binary ULP is less than a millionth of a quote unit.
  requireThat(
    typeof value === 'number' && Number.isFinite(value) && value >= 0 && value < 2 ** 32,
    'invalid number or inadequate cent resolution',
  );
  const r = decimal(String(value));
  requireThat((r.n * 100n) % r.d === 0n, 'output is not on cent grid');
  return (r.n * 100n) / r.d;
}
function priceVolume(row, ticker) {
  // The existing spot seed treats native empty strings as zero, without a tie exception.
  const price = decimal(row.price === '' ? '0' : row.price);
  const volume = decimal(row.volume_24h === '' ? '0' : row.volume_24h);
  requireThat(
    ticker.last === Number(row.price) && ticker.volume === Number(row.volume_24h),
    'price or base volume differs from native snapshot',
  );
  return { n: price.n * volume.n, d: price.d * volume.d };
}
function sameOtherFields(left, right) {
  const keys = Reflect.ownKeys(left);
  return (
    keys.length === Reflect.ownKeys(right).length &&
    keys.every(
      (key) => Object.hasOwn(right, key) && (key === 'quoteVolume' || isDeepStrictEqual(left[key], right[key])),
    )
  );
}

/** Pure ticker comparison; the caller must still run every other ordinary gate check. */
export function compareCoinbaseSpotMidpoints({ venue, records, ts, rust }) {
  try {
    for (const output of [ts, rust]) {
      requireThat(
        output &&
          !Object.hasOwn(output, 'account') &&
          (output.commands === undefined || (Array.isArray(output.commands) && output.commands.length === 0)),
        'account or command output excluded',
      );
    }
    const { snapshot, ordinal, time, candidates } = publicCatalog(venue, records);
    const left = unique(ts.tickers, 'symbol'),
      right = unique(rust.tickers, 'symbol');
    const markets = [unique(ts.markets, 'id'), unique(rust.markets, 'id')];
    requireThat(
      [left, right, ...markets].every(
        (rows) => rows.size === snapshot.size && [...snapshot.keys()].every((id) => rows.has(id)),
      ),
      'catalog/output identity sets differ',
    );
    const exceptions = [];
    for (const [id, raw] of snapshot) {
      for (const rows of markets) {
        const market = rows.get(id);
        requireThat(
          market.symbol === id &&
            market.base === raw.base_currency_id &&
            market.quote === raw.quote_currency_id &&
            market.isSpot === true &&
            market.isInverse === false &&
            market.isExpiring === false &&
            market.contractSize === 1,
          'market identity or contract units differ',
        );
      }
      requireThat(isDeepStrictEqual(markets[0].get(id), markets[1].get(id)), 'other market fields differ');
      const a = left.get(id),
        b = right.get(id);
      requireThat(sameOtherFields(a, b), 'other ticker fields differ');
      const value = priceVolume(raw, a);
      priceVolume(raw, b);
      if (value.n < 1000n * value.d) {
        requireThat(isDeepStrictEqual(a, b), 'sub1000 quote volume mismatch');
        continue;
      }
      const lower = (value.n * 100n) / value.d;
      const remainder = (value.n * 100n) % value.d;
      const midpoint = remainder * 2n === value.d;
      const ac = cents(a.quoteVolume),
        bc = cents(b.quoteVolume);
      // Also reject matching, wrong quote volumes in either model at cent precision.
      const nearest = lower + (remainder * 2n >= value.d ? 1n : 0n);
      requireThat(
        [ac, bc].every((c) => c === nearest || (midpoint && c === lower)),
        'quote volume is not nearest to independent decimal notional',
      );
      if (ac === bc) continue;
      requireThat(
        midpoint && ac !== bc && [ac, bc].every((c) => c === lower || c === lower + 1n),
        'not distinct midpoint neighbors',
      );
      const native = decimal(raw.approximate_quote_24h_volume);
      requireThat(
        (native.n * 100n) % native.d === 0n &&
          [(native.n * 100n) / native.d].some((c) => c === lower || c === lower + 1n),
        'native approximate quote volume does not corroborate midpoint',
      );
      // A tape's last response need not have been consumed. Every possible
      // source of these observed numeric fields must prove the same notional.
      const candidateRequestOrdinals = [];
      for (const candidate of candidates) {
        const row = candidate.snapshot.get(id);
        if (!row || Number(row.price) !== a.last || Number(row.volume_24h) !== a.volume) continue;
        requireThat(
          row.base_currency_id === markets[0].get(id).base && row.quote_currency_id === markets[0].get(id).quote,
          'ambiguous candidate product units',
        );
        const product = priceVolume(row, a);
        requireThat(product.n * value.d === value.n * product.d, 'ambiguous candidate decimal notional');
        const estimate = decimal(row.approximate_quote_24h_volume);
        requireThat(
          (estimate.n * 100n) % estimate.d === 0n &&
            [(estimate.n * 100n) / estimate.d].some((c) => c === lower || c === lower + 1n),
          'candidate native estimate does not corroborate midpoint',
        );
        candidateRequestOrdinals.push(candidate.ordinal);
      }
      requireThat(candidateRequestOrdinals.length > 0, 'no unambiguous native candidate');
      exceptions.push({
        symbol: id,
        ts: a.quoteVolume,
        rust: b.quoteVolume,
        native: raw.approximate_quote_24h_volume,
        notional: { numerator: String(value.n), denominator: String(value.d) },
        requestOrdinal: ordinal,
        timestamp: time,
        candidateRequestOrdinals,
      });
    }
    requireThat(exceptions.length > 0, 'no midpoint representation difference');
    return { equivalent: true, exceptions };
  } catch (error) {
    return { equivalent: false, exceptions: [], reason: error.message };
  }
}
