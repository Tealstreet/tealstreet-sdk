'use strict';
const { load, root, path, assert } = require('./oracle-loader.cjs');
const income = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.income.ts'));
const history = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.order-history.ts'));
const bits = (n) => {
    const b = Buffer.alloc(8);
    b.writeDoubleBE(n);
    return Number.isNaN(n) ? '7ff8000000000000' : b.toString('hex');
};
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') return { bits: bits(v) };
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, obs(x)]));
    return v;
}
const base = {
    symbol: 'BTCUSDT',
    ctime: '1700000000123',
    mtime: '1700000000456',
    tradeId: 'T',
    orderId: 'O',
    side: 'SELL',
    orderType: 'MARKET',
    status: 'FILLED',
    price: 'MARKET',
    avgPrice: '12.5',
    qty: '3.25',
    tradeQty: '1.5',
    closePrice: '13.25',
    entryPrice: '10.75',
    realizedPNL: '2.125',
    fee: '0.375',
    leverage: '9.25',
    marginCoin: 'USDC',
    reduceOnly: true,
};
const fixtures = [
    base,
    null,
    {},
    ...[
        'ctime',
        'mtime',
        'symbol',
        'tradeId',
        'orderId',
        'side',
        'orderType',
        'status',
        'price',
        'avgPrice',
        'qty',
        'tradeQty',
        'closePrice',
        'entryPrice',
        'realizedPNL',
        'fee',
        'leverage',
        'marginCoin',
        'reduceOnly',
    ].flatMap((k) =>
        [null, '', '-0tail', 'Infinity', '-2.25tail', false, {}, ['4.5']].map((v) => ({ ...base, [k]: v })),
    ),
    ...['BUY', 'LONG', 'sell'].map((side) => ({ ...base, side })),
    ...['INIT', 'NEW', 'PART_FILLED', 'CANCELED', 'CANCELLED', 'EXPIRED', 'REJECTED', 'unknown'].map((status) => ({
        ...base,
        status,
    })),
    { ...base, ctime: '1'.repeat(310) },
    { ...base, ctime: '9007199254740993' },
    { ...base, mtime: 'bad' },
    { ...base, fee: '-0.375' },
    { ...base, realizedPNL: '-0' },
    { ...base, qty: '5e-324' },
    { ...base, qty: '1e308', closePrice: '1e308' },
    { ...base, tradeId: false },
    { ...base, fee: { toString: null } },
    { ...base, reduceOnly: 'TRUE' },
];
const rows = [];
for (const [key, fn] of Object.entries({
    trade: income.mapBitunixTradeToIncomeRecords,
    closed: (r) => income.mapBitunixClosedPosition(r, 'acct'),
    order: history.mapBitunixHistoricalOrder,
}))
    for (const row of fixtures) {
        let result;
        try {
            result = { ok: obs(fn(row)) };
        } catch (e) {
            result = { error: e.name, message: e.message };
        }
        rows.push({ key, raw: JSON.stringify(row), result });
    }
assert.equal(fixtures.length, 176, 'History mapper independent fixture floor');
assert.equal(rows.length, 528, 'History mapper independent case floor');
process.stdout.write(JSON.stringify(rows));
