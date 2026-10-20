// A fixer may repair only these documented reference fields. Permissions match
// complete leaf paths: [] is one array index and {name} one object-map key.
// There are deliberately no prefix permissions or recursive wildcards.
import { isDeepStrictEqual } from 'node:util';

const rowFields = (base, fields) => fields.map((field) => `${base}[].${field}`);
const fillFields = ['id', 'symbol', 'side', 'amount', 'price'];
const queryFields = [
  'id', 'timestamp', 'symbol', 'orderId', 'price', 'qty', 'side', 'commission',
  'commissionAsset', 'realizedPnl', 'maker', 'type', 'status', 'averagePrice',
  'amount', 'filled', 'reduceOnly', 'createdTime', 'updatedTime', 'incomeType',
  'income', 'asset', 'tranId', 'category', 'quoteQty',
];
const orderRemovalFields = [
  'id', 'clientOrderId', 'symbol', 'status', 'type', 'side', 'price',
  'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId', 'amount',
  'filled', 'remainingNative', 'timeInForce', 'remaining', 'reduceOnly', 'close',
  'trigger', 'rejectReason',
];
const fills = rowFields('account.fills', fillFields);
const queryLeaves = rowFields('commands[].ok', queryFields);
const queryArrays = [{ path: 'commands[].ok', add: true, remove: true }];
const fillArrays = [{ path: 'account.fills', add: true, remove: true, reorder: true }];
const requestArrays = [{ path: 'requests', add: true, remove: true, reorder: true }];
const subscriptionArrays = [{ path: 'subscriptions', add: true, remove: true, reorder: true }];
const bookFields = ['books.{symbol}.bids[][]', 'books.{symbol}.asks[][]'];
const bookArrays = ['bids', 'asks'].flatMap((side) => [
  { path: `books.{symbol}.${side}`, add: true, remove: true, reorder: true },
  { path: `books.{symbol}.${side}[]`, add: true, remove: true },
]);
const contract = (fields = [], arrays = [], nulls = []) => ({ fields, arrays, nulls });
const bybit = [
  'account.positions[].marginSize', 'account.orders[].rejectReason',
  'requests[]', 'subscriptions[]',
];
const coinbase = contract([
  'markets[].active', 'requests[]', ...fills,
  ...rowFields('account.orders', ['clientOrderId', 'price', 'remaining', 'amount']),
  'account.balance.upnl', 'account.balance.currencies.{currency}.total',
  'account.settings[].leverage', ...queryLeaves, 'commands[].error',
  'commands[].ok.buyLeverage', 'commands[].ok.sellLeverage',
], [...requestArrays, ...fillArrays, ...queryArrays], ['commands[].ok']);
const orderly = contract([
  'tickers[].percentage', 'requests[]', ...fills,
  ...rowFields('account.orders', ['type', 'clientOrderId', 'timeInForce', 'trigger', 'price', 'status']),
  ...rowFields('account.settings', ['symbol', 'leverage', 'marginType']),
  ...queryLeaves, 'commands[].error', 'commands[].ok.buyLeverage',
  'commands[].ok.sellLeverage', 'commands[].ok[]',
], [
  ...requestArrays, ...fillArrays, ...queryArrays,
  { path: 'account.orders', remove: true, key: 'id', removedFields: orderRemovalFields },
]);
const lighter = contract([...fills, 'commands[].error'], fillArrays, ['commands[].ok']);
const deepFreeze = (value) => {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) deepFreeze(child);
    Object.freeze(value);
  }
  return value;
};

// Empty contracts are intentional: adding a new fixer or a new field requires
// a reviewed declaration here. In particular, rebuilt fills cannot delete the
// newly compared executionId, fee or timestamp, even when their value is null.
export const FIXER_CONTRACTS = deepFreeze({
  binance: contract(['account.positions[].marginSize']),
  binancepm: contract(['account.positions[].marginSize']),
  binanceinverse: contract(['account.orders[].status']),
  binancespot: contract(['account.orders[].status', 'account.fills[].price']),
  bybit: contract(bybit, [...requestArrays, ...subscriptionArrays]),
  bybitdemo: contract(bybit, [...requestArrays, ...subscriptionArrays]),
  hyrotrader: contract(bybit, [...requestArrays, ...subscriptionArrays]),
  bybitinverse: contract([
    ...bybit, 'account.balance.total', 'account.balance.upnl',
  ], [...requestArrays, ...subscriptionArrays]),
  bybitspot: contract([
    ...bybit, 'markets[].precision.amount', 'markets[].limits.amount.max',
  ], [...requestArrays, ...subscriptionArrays]),
  blofin: contract(['account.positions[].marginSize', 'account.balance.used', 'account.balance.upnl']),
  weex: contract(['account.orders[].type', 'account.orders[].price']),
  phemex: contract([
    'tickers[].percentage', 'account.orders[].filled', 'account.orders[].remaining',
    ...rowFields('account.settings', ['symbol', 'leverage', 'marginType']), ...fills,
  ], [
    ...fillArrays,
    { path: 'account.settings', add: true, key: 'symbol', reorder: true },
  ]),
  bingx: contract([
    'markets[].precision.amount', 'markets[].precision.price',
    ...rowFields('tickers', ['percentage', 'volume', 'quoteVolume', 'bid', 'ask']),
    ...bookFields, 'account.orders[].close', ...fills,
  ], [...bookArrays, ...fillArrays]),
  backpack: contract([
    ...rowFields('tickers', ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume']),
    'account.hedged',
    ...rowFields('account.orders', ['clientOrderId', 'timeInForce', 'amount', 'remaining', 'status', 'trigger']),
    'account.positions[].entryPrice', ...fills,
  ], fillArrays),
  cryptocom: contract([
    'account.hedged', 'subscriptions[]',
    ...rowFields('tickers', ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']),
    ...fills,
  ], [...subscriptionArrays, ...fillArrays]),
  lighter,
  'lighter-robinhood': lighter,
  kraken: contract([
    'markets[].precision.amount',
    ...rowFields('tickers', ['fundingRate', 'volume', 'quoteVolume', 'openInterest', 'percentage']),
    ...bookFields, 'account.balance.coins.{currency}.amount',
    'account.balance.coins.{currency}.notional', 'account.positions[].marginSize',
    'requests[]', 'subscriptions[]', ...fills,
  ], [...bookArrays, ...requestArrays, ...subscriptionArrays, ...fillArrays]),
  coinbase,
  coinbasespot: coinbase,
  orderly,
  'tealstreet-dex': orderly,
  kodiakfi: orderly,
  mode: orderly,
  salsadex: orderly,
  what: orderly,
  woofi: orderly,
  kucoin: contract(['requests[]', ...queryLeaves, 'commands[].error'], [...requestArrays, ...queryArrays]),
});

const parsePath = (path) => path.match(/\[\]|\{[^}]+\}|[^.[\]]+/g) ?? [];
const matches = (pattern, path) => {
  const tokens = parsePath(pattern);
  return tokens.length === path.length && tokens.every((token, index) =>
    token === '[]' ? typeof path[index] === 'number'
      : token.startsWith('{') ? typeof path[index] === 'string'
        : token === path[index]);
};
const pathString = (path) => path.reduce((out, part) => typeof part === 'number'
  ? `${out}[${part}]`
  : /^[A-Za-z_$][\w$]*$/.test(part) ? `${out}${out ? '.' : ''}${part}`
    : `${out}[${JSON.stringify(part)}]`, '') || '<root>';
const isContainer = (value) => value !== null && typeof value === 'object';
const kind = (value, present) => !present ? 'missing' : Array.isArray(value) ? 'array'
  : value === null ? 'null' : typeof value;
const plain = (value) => !isContainer(value) || Array.isArray(value)
  || Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null;
const ownKeys = (value) => Reflect.ownKeys(value).filter((key) => !Array.isArray(value) || key !== 'length');
const arrayIndex = (key) => typeof key === 'string' && /^(0|[1-9]\d*)$/.test(key)
  && Number(key) < 4294967295 ? Number(key) : key;

/**
 * Audit the entire TS correction boundary, including imported helpers.
 * `before` must be a detached pre-fixer snapshot; neither input is mutated.
 * Counts contain changed leaf paths plus @type/@length structural changes.
 * Unknown venues have no permissions. No candidate output enters this API.
 */
export function auditFixerChanges(venue, before, after) {
  const permission = Object.hasOwn(FIXER_CONTRACTS, venue) ? FIXER_CONTRACTS[venue] : contract();
  const errors = [];
  const counts = {};
  let total = 0;
  const beforeStack = new Set(), afterStack = new Set();
  const record = (path, allowed, suffix = '') => {
    const label = pathString(path) + suffix;
    Object.defineProperty(counts, label, { value: (Object.hasOwn(counts, label) ? counts[label] : 0) + 1, enumerable: true, configurable: true });
    total++;
    if (!allowed) errors.push(`${venue}: fixer changed undeclared field ${label}`);
  };
  const arrayRule = (path) => permission.arrays.find((rule) => matches(rule.path, path));
  const allowedLeaf = (path, value, removal) => removal
    ? path.length === removal.path.length + 1
      && removal.path.every((part, index) => part === path[index])
      && removal.fields.includes(path.at(-1))
    : permission.fields.some((pattern) => matches(pattern, path))
      || (value === null && permission.nulls.some((pattern) => matches(pattern, path)));
  const allowedContainer = (path, value, present, removal, seen = new Set()) => {
    if (!present) return false;
    if (Array.isArray(value)) return Boolean(arrayRule(path));
    if (!plain(value) || seen.has(value)) return false;
    // Nonempty row/map containers are permitted only when at least one precise
    // descendant leaf is permitted; every descendant is audited independently.
    seen.add(value);
    try {
      return ownKeys(value).some((key) => typeof key === 'string' && (
        !isContainer(value[key]) ? allowedLeaf([...path, key], value[key], removal)
          : allowedContainer([...path, key], value[key], true, removal, seen)));
    } finally {
      seen.delete(value);
    }
  };
  const visit = (left, leftPresent, right, rightPresent, path, removal) => {
    if (leftPresent === rightPresent && Object.is(left, right)) return;
    const leftContainer = leftPresent && isContainer(left), rightContainer = rightPresent && isContainer(right);
    if ((leftContainer && (!plain(left) || beforeStack.has(left)))
      || (rightContainer && (!plain(right) || afterStack.has(right)))) {
      record(path, false, '@unsupported');
      return;
    }
    if (!leftContainer && !rightContainer) {
      const value = rightPresent ? right : left;
      record(path, allowedLeaf(path, value, removal));
      return;
    }
    const leftKind = kind(left, leftPresent), rightKind = kind(right, rightPresent);
    if (leftKind !== rightKind) {
      const container = rightContainer ? right : left;
      const scalarAllowed = rightContainer
        ? (!leftPresent || (left === null && permission.nulls.some((pattern) => matches(pattern, path))))
        : (!rightPresent || (right === null && permission.nulls.some((pattern) => matches(pattern, path))));
      const rule = Array.isArray(container) ? arrayRule(path) : undefined;
      const directionAllowed = !Array.isArray(container) || Boolean(rightContainer ? rule?.add : rule?.remove);
      record(path, scalarAllowed && directionAllowed && allowedContainer(path, container, true, removal), '@type');
    }
    if (leftContainer) beforeStack.add(left);
    if (rightContainer) afterStack.add(right);
    try {
      if (Array.isArray(left) && Array.isArray(right)) {
        const rule = arrayRule(path);
        if (left.length !== right.length) record(path, Boolean(rule && (right.length > left.length ? rule.add : rule.remove)), '@length');
        const keyed = (rows) => {
          if (!rule?.key) return undefined;
          const map = new Map();
          for (const [index, row] of rows.entries()) {
            if (!row || typeof row !== 'object' || !Object.hasOwn(row, rule.key)
              || !['string', 'number'].includes(typeof row[rule.key]) || map.has(row[rule.key])) return undefined;
            map.set(row[rule.key], { row, index });
          }
          return map;
        };
        const leftMap = keyed(left), rightMap = keyed(right);
        if (leftMap && rightMap) {
          const oldCommon = [...leftMap.keys()].filter((key) => rightMap.has(key));
          const newCommon = [...rightMap.keys()].filter((key) => leftMap.has(key));
          if (oldCommon.some((key, index) => !Object.is(key, newCommon[index]))) record(path, Boolean(rule.reorder), '@order');
          for (const key of new Set([...leftMap.keys(), ...rightMap.keys()])) {
            const old = leftMap.get(key), next = rightMap.get(key);
            const rowPath = [...path, next?.index ?? old.index];
            if (!old || !next) record(rowPath, Boolean(!old ? rule.add : rule.remove), '@row');
            visit(old?.row, Boolean(old), next?.row, Boolean(next), rowPath,
              !next && rule.removedFields ? { path: rowPath, fields: rule.removedFields } : removal);
          }
          // Non-index own properties are never covered by a row permission.
          for (const key of new Set([...ownKeys(left), ...ownKeys(right)])) {
            if (typeof arrayIndex(key) === 'number') continue;
            if (typeof key === 'symbol') record(path, false, '@symbol');
            else visit(left[key], Object.hasOwn(left, key), right[key], Object.hasOwn(right, key), [...path, key], removal);
          }
          return;
        }
        // Rows without a stable key still cannot evade reorder permissions.
        // Match unchanged rows with multiplicity, including when other rows
        // were added/removed; reordered common rows must be declared explicitly.
        if (!isDeepStrictEqual(left, right)) {
          const matched = new Set();
          let lastMatch = -1, reordered = false;
          for (const row of left) {
            const index = right.findIndex((candidate, next) => !matched.has(next) && isDeepStrictEqual(row, candidate));
            if (index < 0) continue;
            matched.add(index);
            if (index < lastMatch) reordered = true;
            lastMatch = index;
          }
          if (reordered) record(path, Boolean(rule?.reorder), '@order');
        }
      }
      const keys = new Set([
        ...(leftContainer ? ownKeys(left) : []),
        ...(rightContainer ? ownKeys(right) : []),
      ]);
      for (const key of keys) {
        if (typeof key === 'symbol') { record(path, false, '@symbol'); continue; }
        const leftOwn = leftContainer && Object.hasOwn(left, key), rightOwn = rightContainer && Object.hasOwn(right, key);
        const part = Array.isArray(left) || Array.isArray(right) ? arrayIndex(key) : key;
        if (typeof part === 'number' && leftOwn !== rightOwn) {
          const rule = arrayRule(path);
          record([...path, part], Boolean(!leftOwn ? rule?.add : rule?.remove), '@row');
          if ((!leftOwn && Array.isArray(left) && part < left.length)
            || (!rightOwn && Array.isArray(right) && part < right.length)) record([...path, part], false, '@hole');
        }
        visit(leftOwn ? left[key] : undefined, leftOwn, rightOwn ? right[key] : undefined, rightOwn, [...path, part], removal);
      }
      // A sparse new tail changes shape even though it has no enumerable leaf.
      if (Array.isArray(left) && Array.isArray(right)) {
        for (let index = Math.min(left.length, right.length); index < Math.max(left.length, right.length); index++) {
          const array = index < right.length ? right : left;
          if (!Object.hasOwn(array, index)) record([...path, index], false, '@hole');
        }
      }
    } finally {
      if (leftContainer) beforeStack.delete(left);
      if (rightContainer) afterStack.delete(right);
    }
  };
  visit(before, true, after, true, []);
  return { errors, counts, total };
}
