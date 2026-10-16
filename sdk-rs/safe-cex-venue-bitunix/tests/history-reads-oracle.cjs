const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const names = [
    'fetchBitunixHistoryPages',
    'fetchIncomeHistory',
    'fetchClosedPositionRows',
    'fetchClosedPnl',
    'fetchHistoricalPositions',
    'fetchHistoricalOrders',
];
const functions = Object.fromEntries(
    names.map((name) => {
        const m = cls.members.filter((n) => n.name?.getText(ast) === name);
        assert.equal(m.length, 1);
        return [
            name,
            ts.transpileModule('(function(){return (' + m[0].initializer.getText(ast) + ');})', {
                compilerOptions: { target: ts.ScriptTarget.ES2022 },
            }).outputText,
        ];
    }),
);
const { ENDPOINTS } = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const { LogSeverity } = load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
const income = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.income.ts'));
const orders = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.order-history.ts'));
const position = load(path.join(root, 'packages/safe-cex/src/journal/bitunix.position-adapter.ts'));
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return { bits: Number.isNaN(v) ? '7ff8000000000000' : b.toString('hex') };
    }
    if (typeof v === 'string') return { units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return { entries: Object.entries(v).map(([k, v]) => [k, obs(v)]) };
    return v;
}
function decode(v) {
    if (v && typeof v === 'object' && v.$bits) return Buffer.from(v.$bits, 'hex').readDoubleBE();
    if (v && typeof v === 'object' && v.$units) return String.fromCharCode(...v.$units);
    if (Array.isArray(v)) return v.map(decode);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, v]) => [k, decode(v)]));
    return v;
}
const rows = [];
async function run(name, f) {
    if (process.argv[2] && name !== process.argv[2]) return;
    const events = [],
        mapped = [];
    let rawId = 0,
        call = 0,
        optionReads = 0;
    const raw = new Map(),
        errorToken = { message: 'boom', response: { data: { msg: 'venue error' } } };
    const note = (name, ...args) => {
        events.push([name, ...args.map(obs)]);
        if (f.fail === name) throw errorToken;
    };
    const makeRow = (p, i) => {
        const row = {
            ctime: String(1000 - p * 100 - i),
            mtime: String(2000 - p * 100 - i),
            tradeId: 'T' + (p * 100 + i),
            positionId: 'P' + (p * 100 + i),
            orderId: 'O' + (p * 100 + i),
            symbol: 'A',
            side: 'BUY',
            orderType: 'LIMIT',
            qty: '2',
            maxQty: '3',
            closePrice: '12',
            entryPrice: '10',
            realizedPNL: '4',
            fee: '1',
            funding: '0.5',
            leverage: '5',
            price: '10',
            tradeQty: '1',
            marginCoin: 'USDT',
            ...decode(f.row ?? {}),
        };
        raw.set(row, rawId++);
        return row;
    };
    const pages = f.pages
        ? f.pages.map((page) =>
              page.map((row) => {
                  const r = decode(row);
                  raw.set(r, rawId++);
                  return r;
              }),
          )
        : null;
    const makePage = (p) => pages?.[p] ?? Array.from({ length: f.full ? 100 : (f.size ?? 3) }, (_, i) => makeRow(p, i));
    const rowId = (r) => raw.get(r);
    const wrapMap =
        (kind, fn) =>
        (r, ...args) => {
            note('map', kind, rowId(r), ...args.filter((v) => typeof v !== 'object'));
            const result = fn(r, ...args);
            mapped.push({
                kind,
                row: rowId(r),
                args: args.filter((v) => typeof v !== 'object').map(obs),
                result: obs(result),
            });
            return result;
        };
    const context = {
        ENDPOINTS,
        LogSeverity,
        omitUndefined: (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)),
        mapBitunixTradeToIncomeRecords: wrapMap('income', income.mapBitunixTradeToIncomeRecords),
        mapBitunixClosedPosition: wrapMap('closed', income.mapBitunixClosedPosition),
        mapBitunixHistoricalOrder: wrapMap('order', orders.mapBitunixHistoricalOrder),
        mapBitunixPositionHistory: (r, deps) => {
            note('map', 'position', rowId(r), deps.account);
            const original = deps.resolveSettleCurrency;
            deps.resolveSettleCurrency = (symbol) => {
                const q = original(symbol);
                note('resolved', symbol, q);
                return q;
            };
            const result = position.mapBitunixPositionHistory(r, deps);
            mapped.push({ kind: 'position', row: rowId(r), args: [obs(deps.account)], result: obs(result) });
            return result;
        },
    };
    const options = decode(f.options ?? {});
    const opts = new Proxy(options, {
        get: (o, k) => {
            note('option', String(k));
            optionReads++;
            return o[k];
        },
    });
    const owner = {
        get isPublicDataOnly() {
            note('public');
            return f.public ?? false;
        },
        emitAndLogError: (message) => note('report', message),
        log: (message, severity) => note('warning', message, severity),
        store: {
            marketsBySymbol: new Proxy(
                {},
                {
                    get: (_, symbol) => {
                        note('quote', symbol);
                        return f.quote === null ? undefined : { quote: decode(f.quote ?? 'USD') };
                    },
                },
            ),
        },
        xhr: {
            get: async (endpoint, config) => {
                const p = call++;
                note('get', endpoint, config);
                if (f.rejectAt === p) throw errorToken;
                const values = makePage(p);
                const result =
                    f.shape === 'none'
                        ? {}
                        : f.shape === 'trade'
                          ? { tradeList: values }
                          : f.shape === 'order'
                            ? { orderList: values }
                            : f.shape === 'priority'
                              ? { positionList: values, tradeList: [], orderList: [] }
                              : f.shape === 'array'
                                ? values
                                : { positionList: values };
                const body = name === 'fetchHistoricalOrders' ? { orderList: values } : result;
                const proxied = Array.isArray(body)
                    ? body
                    : new Proxy(body, {
                          get: (o, k) => {
                              note('field', String(k));
                              return o[k];
                          },
                      });
                return {
                    data: {
                        get data() {
                            note('body');
                            return proxied;
                        },
                    },
                };
            },
        },
    };
    for (const [n, code] of Object.entries(functions)) owner[n] = vm.runInNewContext(code, context).call(owner);
    if (!['fetchBitunixHistoryPages', 'fetchHistoricalOrders'].includes(name)) {
        owner.fetchBitunixHistoryPages = async (endpoint, from, to, timeOf, idOf, symbol) => {
            note('delegate', endpoint, from, to, symbol);
            const values = makePage(0);
            for (const r of values) {
                note('selectorTime', rowId(r), timeOf(r));
                note('selectorId', rowId(r), idOf(r));
            }
            return values;
        };
        if (name === 'fetchHistoricalPositions')
            owner.fetchClosedPositionRows = async (options) => {
                note('closedRows', options);
                return makePage(0);
            };
    }
    let result, error;
    try {
        if (name === 'fetchBitunixHistoryPages')
            result = await owner[name](
                'EP',
                decode(f.start),
                decode(f.end),
                (r) => {
                    note('time', rowId(r));
                    return r.ts ?? Number(r.ctime);
                },
                (r) => {
                    note('id', rowId(r));
                    return String(r.id ?? r.tradeId ?? '');
                },
                decode(f.symbol),
            );
        else result = await owner[name](opts);
    } catch (e) {
        error = e === errorToken ? { opaque: true } : { name: e.name, message: e.message };
    }
    rows.push({
        name,
        fixture: f,
        events,
        result: ['fetchBitunixHistoryPages', 'fetchClosedPositionRows'].includes(name)
            ? result?.map(rowId)
            : obs(result),
        error: error ?? null,
        raw: [...raw.entries()].map(([r, id]) => ({ id, row: obs(r) })),
        mapped,
    });
}
(async () => {
    for (const f of [
        {},
        { shape: 'array' },
        { shape: 'trade' },
        { shape: 'order' },
        { shape: 'priority' },
        { shape: 'none' },
        { size: 0 },
        { start: 999, end: 1000 },
        { start: 0, end: 0 },
        { start: { $bits: '7ff8000000000000' }, end: { $bits: '7ff0000000000000' } },
        { row: { ts: 0, id: '' }, start: 2000, end: 1 },
        { row: { ts: { $bits: '7ff8000000000000' }, id: { $units: [0xd800, 65] } } },
        {
            full: true,
            pages: [
                Array.from({ length: 100 }, (_, i) => ({ ctime: '1000', id: 'I' + i })),
                [
                    { ctime: '999', id: 'I0' },
                    { ctime: '998', id: 'NEW' },
                ],
            ],
        },
        { full: true, row: { tradeId: '' }, rejectAt: 1 },
        { full: true, row: { tradeId: 'D' } },
        { full: true, start: 995 },
        { full: true },
        { fail: 'time' },
        { fail: 'id' },
        { fail: 'warning', full: true },
        { fail: 'report', rejectAt: 0 },
        { rejectAt: 0 },
    ])
        await run('fetchBitunixHistoryPages', f);
    for (const name of ['fetchIncomeHistory', 'fetchClosedPositionRows', 'fetchClosedPnl', 'fetchHistoricalPositions'])
        for (const f of [
            {},
            { public: true },
            {
                options: {
                    from: 10,
                    to: 20,
                    symbol: 'A',
                    startTime: 11,
                    endTime: 21,
                    accountName: 'AC',
                    account: 'ACC',
                },
            },
            { row: { ctime: 'bad', mtime: '', symbol: 'A' } },
            { row: { mtime: null, ctime: { $units: [49, 50, 0xd800] }, fee: '-2' } },
            { size: 0 },
            { fail: 'delegate' },
            { fail: 'map' },
            { quote: '', row: { marginCoin: '' } },
            { quote: null, row: { marginCoin: '' } },
            { options: { accountName: { $units: [0xd800] }, account: { $units: [0xd800] } } },
        ])
            await run(name, f);
    for (const f of [
        {},
        { public: true },
        { size: 0 },
        { options: { limit: 1.9 } },
        { options: { limit: 0 } },
        { options: { limit: { $bits: '7ff0000000000000' } } },
        { row: { orderId: '', ctime: '1' } },
        { row: { ctime: 'bad' } },
        { full: true, row: { side: 'UNKNOWN' } },
        { full: true, row: { orderId: 'D' } },
        { full: true, options: { from: 995 } },
        { full: true },
        { full: true, rejectAt: 1 },
        { rejectAt: 0 },
        { fail: 'map' },
        { fail: 'warning', full: true },
        { fail: 'report', rejectAt: 0 },
    ])
        await run('fetchHistoricalOrders', f);
    assert.equal(
        rows.length,
        process.argv[2]
            ? process.argv[2] === 'fetchBitunixHistoryPages'
                ? 22
                : process.argv[2] === 'fetchHistoricalOrders'
                  ? 17
                  : 11
            : 83,
    );
    process.stdout.write(JSON.stringify({ rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
