const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const names = ['fetchOHLCV', 'updateSymbolMetadata', 'validateAccount'];
const funcs = Object.fromEntries(
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
const { MarginType } = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const actualDayjs = require('dayjs');
function decode(v) {
    if (v && v.$bits) return Buffer.from(v.$bits, 'hex').readDoubleBE();
    if (Array.isArray(v)) return v.map(decode);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, v]) => [k, decode(v)]));
    return v;
}
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return { bits: Number.isNaN(v) ? '7ff8000000000000' : b.toString('hex') };
    }
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return { entries: Object.entries(v).map(([k, v]) => [k, obs(v)]) };
    return v;
}
const rows = [];
(async () => {
    for (const name of names)
        for (const f of name === 'fetchOHLCV'
            ? [
                  {},
                  { options: { limit: 500 } },
                  { options: { limit: 0, from: 1000, to: 5000 } },
                  { options: { limit: -2, to: 5000 } },
                  { options: { limit: { $bits: '7ff8000000000000' } } },
                  { options: { interval: '1M' } },
                  { options: { interval: 'bad' } },
                  { options: { interval: '1h2z' } },
                  { data: [] },
                  { data: null },
                  { data: {} },
                  {
                      data: [
                          { time: '2000', open: '-0', high: 'Infinity', low: 'bad', close: '12.5x', quoteVol: null },
                      ],
                  },
                  { fail: 'get' },
                  { fail: 'date' },
                  { fail: 'subtract' },
              ]
            : name === 'updateSymbolMetadata'
              ? [
                    {},
                    { data: { marginMode: 'ISOLATION', leverage: '7' } },
                    { data: null },
                    { data: false },
                    { data: 0 },
                    { data: {} },
                    { data: { marginMode: 'isolation', leverage: 0 } },
                    { fail: 'get' },
                    { fail: 'set' },
                    { fail: 'update' },
                ]
              : [
                    {},
                    { data: { code: '0', msg: 'bad' } },
                    { data: { code: 1, msg: 17 } },
                    { data: { code: 1, msg: '' } },
                    { data: null },
                    { fail: 'get' },
                    { fail: 'extract' },
                    { fail: 'balance' },
                    { fail: 'report', getFail: true },
                    { data: { code: 0 }, missingMessage: true, getFail: true },
                ]) {
            const events = [],
                leaves = [];
            const note = (name, ...args) => {
                events.push([name, ...args.map(obs)]);
                if (f.fail === name) throw new Error(name + ' failed');
            };
            let config;
            const data = Object.hasOwn(f, 'data')
                ? decode(f.data)
                : name === 'fetchOHLCV'
                  ? [{ time: 2000, open: '1.25', high: '2', low: '1', close: '1.5', quoteVol: '5' }]
                  : name === 'updateSymbolMetadata'
                    ? { marginMode: 'CROSS', leverage: 3 }
                    : { code: 0 };
            const original = [
                { id: 'a', symbol: 'A', leverage: 1 },
                { id: 'b', symbol: 'B', leverage: 2 },
            ];
            let updated;
            const owner = {
                xhr: {
                    get: async (endpoint, options) => {
                        config = options;
                        note('get', endpoint, options);
                        if (f.getFail) throw { message: f.missingMessage ? undefined : 'get failed' };
                        return { data: name === 'validateAccount' ? data : { data } };
                    },
                },
                symbolSettings: { set: (symbol, settings) => note('set', symbol, settings) },
                store: {
                    get positions() {
                        note('positions');
                        return original;
                    },
                    update: (value) => {
                        note('update', value);
                        updated = value.positions;
                    },
                },
                extractAccountRow: (body) => {
                    note('extract', body);
                },
                emitAndLogError: (msg) => note('report', msg),
                fetchBalance: async () => {
                    note('balance');
                    return { balance: { total: 12.5, free: 3 } };
                },
            };
            const wrap = (d) => ({
                subtract: (amount, unit) => {
                    note('subtract', amount, unit);
                    const result = d.subtract(amount, unit);
                    leaves.push({ value: obs(result.valueOf()) });
                    return wrap(result);
                },
                valueOf: () => {
                    note('valueOf', d.valueOf());
                    return d.valueOf();
                },
            });
            const dayjs = (v) => {
                note('date', v);
                const d = actualDayjs(v === undefined ? 1700000000000 : v);
                leaves.push({ value: obs(d.valueOf()) });
                return wrap(d);
            };
            let result, error;
            try {
                const fn = vm.runInNewContext(funcs[name], { ENDPOINTS, MarginType, dayjs }).call(owner);
                result = await (name === 'fetchOHLCV'
                    ? fn({ ...decode({ symbol: 'A', interval: '1m', ...f.options }) })
                    : name === 'updateSymbolMetadata'
                      ? fn('A')
                      : fn());
            } catch (e) {
                error = { name: e.name, message: e.message };
            }
            rows.push({
                name,
                fixture: f,
                events,
                data,
                leaves,
                result: obs(result),
                error: error ?? null,
                identity: updated ? updated.map((p, i) => p === original[i]) : null,
            });
        }
    assert.equal(rows.length, 35);
    process.stdout.write(JSON.stringify({ rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
