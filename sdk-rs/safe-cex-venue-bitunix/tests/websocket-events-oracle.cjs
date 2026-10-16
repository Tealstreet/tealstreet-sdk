const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const methods = {};
for (const [scope, names] of Object.entries({
    public: ['onOpen', 'onMessage', 'handleTickerStreamEvent', 'handlePriceStreamEvent', 'handleDepthBookStreamEvent'],
    futures: ['onOpen', 'onMessage'],
})) {
    const file = path.join(root, `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-${scope}.ts`),
        ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
        cls = ast.statements.find(ts.isClassDeclaration);
    for (const name of names) {
        const members = cls.members.filter((n) => n.name?.getText(ast) === name);
        assert.equal(members.length, 1);
        methods[scope + '.' + name] = ts.transpileModule(
            '(function(){return (' + members[0].initializer.getText(ast) + ');})',
            { compilerOptions: { target: ts.ScriptTarget.ES2022 } },
        ).outputText;
    }
}
const keyFile = path.join(root, 'packages/safe-cex/src/utils/get-key.ts'),
    keyModule = { exports: {} };
vm.runInThisContext(
    '(function(require,module,exports){' +
        ts.transpileModule(fs.readFileSync(keyFile, 'utf8'), {
            compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true },
        }).outputText +
        '\n})',
)(
    (id) => {
        assert(['lodash/camelCase', 'lodash/memoize', 'lodash/snakeCase'].includes(id));
        return require('' + id);
    },
    keyModule,
    keyModule.exports,
);
const { jsonParse } = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const pako = require('pako');
function num(bits) {
    return Buffer.from(bits, 'hex').readDoubleBE();
}
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return { bits: Number.isNaN(v) ? '7ff8000000000000' : b.toString('hex') };
    }
    if (typeof v === 'string') return { units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
    if (v instanceof Error || (v && v.name?.endsWith('Error') && typeof v.message === 'string'))
        return { error: [v.name, v.message] };
    if (typeof v === 'symbol') return { symbol: v.description };
    if (v instanceof Blob) return { blob: true };
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return { entries: Object.entries(v).map(([k, v]) => [k, obs(v)]) };
    return v;
}
function text(units) {
    return String.fromCharCode(...units);
}
const rows = [];
async function run(key, f) {
    const events = [],
        leaves = [],
        arrays = [],
        timers = [];
    let disposed = f.disposed ?? false,
        secondary = f.secondary ?? false,
        timer = f.timerBits ? num(f.timerBits) : undefined,
        reads = 0,
        count = f.countBits ? num(f.countBits) : 0;
    let parsed, parseError, topicEntries, decoded, readError, lookupKey;
    const errorToken = { tag: 'boom' };
    const note = (name, ...args) => {
        events.push([name, ...args.map(obs)]);
        if (f.fail === name) throw new Error(name + ' failed');
    };
    const list = [];
    let owner;
    function handler(id) {
        return (arg) => {
            if (key.startsWith('public')) {
                assert(Array.isArray(arg) && arg.length === 1);
                assert.equal(arg[0], parsed);
                assert(!arrays.includes(arg));
                arrays.push(arg);
            } else assert.equal(arg, parsed);
            note('call', id, arg);
            if (f.throwHandler) throw errorToken;
            if (id === 'h0' && f.edit === 'append') list.push(handler('h2'));
            if (id === 'h0' && f.edit === 'delete') delete list[1];
        };
    }
    list.push(handler('h0'), handler('h1'));
    const JSONProjection = {
        parse: (data) => {
            note('parse', data);
            try {
                parsed = JSON.parse(data);
                return parsed;
            } catch (e) {
                parseError = obs(e);
                throw e;
            }
        },
        stringify: (value) => {
            note(key.endsWith('onOpen') ? 'frame' : 'key', value);
            const out = JSON.stringify(value);
            lookupKey = out;
            return out;
        },
    };
    const context = {
        JSON: JSONProjection,
        console: { error: (...args) => note('log', ...args) },
        setTimeout: (fn, delay) => {
            note('schedule', fn.name, delay);
            timers.push(fn);
            return 99;
        },
        clearTimeout: (t) => note('clear', t),
        Blob,
        TextDecoder,
        Uint8Array,
        inflate: (data) => {
            try {
                const value = pako.inflate(data);
                decoded = new TextDecoder().decode(value);
                return value;
            } catch (e) {
                readError = obs(e);
                throw e;
            }
        },
        jsonParse: (data) => {
            note('parse', data);
            parsed = jsonParse(data);
            return parsed;
        },
        v: (data, k) => {
            note('field', k);
            return keyModule.exports.v(data, k);
        },
        parseFloat: (value) => {
            note('float', value);
            try {
                const out = Number.parseFloat(value);
                const scalar = String(value);
                leaves.push({
                    value: obs(value),
                    result: obs(out),
                    units: Array.from({ length: scalar.length }, (_, i) => scalar.charCodeAt(i)),
                });
                return out;
            } catch (e) {
                leaves.push({ value: obs(value), error: obs(e) });
                throw e;
            }
        },
    };
    owner = {
        parent: {
            emitter: {
                off: (e, h) => {
                    assert.equal(h, owner.onMarketsChanged);
                    note('off', e, 'onMarketsChanged');
                },
                on: (e, h) => {
                    assert.equal(h, owner.onMarketsChanged);
                    note('on', e, 'onMarketsChanged');
                },
            },
        },
        onMarketsChanged: () => {},
        resetOrderbook: () => note('reset'),
        subscribeMarketsToChannel: (channel) => {
            note('markets', channel);
            if (f.flipSecondary && channel === 'ticker') secondary = true;
        },
        subscribe: () => note('subscribe'),
        ping: () => {
            note('ping');
            if (f.disposeAtPing) disposed = true;
        },
        send: (wire) => note('send', wire),
        trackLastReceived: () => note('track'),
        recordMessageHandlerStats: (t) => note('stats', t),
        handleTickerStreamEvent: (p) => {
            assert.equal(p, parsed);
            note('stream', 'ticker', p);
        },
        handlePriceStreamEvent: (p) => {
            assert.equal(p, parsed);
            note('stream', 'price', p);
        },
        handleDepthBookStreamEvent: (p) => {
            assert.equal(p, parsed);
            note('stream', 'depth_book1', p);
        },
    };
    Object.defineProperties(owner, {
        isDisposed: {
            get() {
                note('disposed', disposed);
                return disposed;
            },
        },
        isSecondary: {
            get() {
                note('secondary', secondary);
                return secondary;
            },
        },
        pingLoopTimeoutId: {
            get() {
                const v = f.timerReads?.[reads++] ?? timer;
                note('timer', v);
                return v;
            },
            set(v) {
                note('setTimer', v);
                timer = v;
            },
        },
        messageCount: {
            get() {
                note('count', count);
                return count;
            },
            set(v) {
                note('setCount', v);
                count = v;
            },
        },
        topics: {
            get() {
                note('topics');
                const topics = f.topics ?? { 2: 1, positive: 2, zero: 0, negative: -1 };
                topicEntries = Object.entries(topics).map(([k, v]) => [obs(k), obs(+v)]);
                return topics;
            },
        },
        messageHandlers: {
            get() {
                return new Proxy(
                    {},
                    {
                        get: (_, channel) => {
                            note('handlers', String(channel));
                            return f.noHandlers ? undefined : list;
                        },
                    },
                );
            },
        },
    });
    let input, error, result;
    try {
        if (key.endsWith('onOpen')) {
            vm.runInNewContext(methods[key], context).call(owner)();
            if (f.repeat && timers[0]) {
                timers[0]();
                if (f.disposeBeforeRepeat) {
                    disposed = true;
                    timers[0]();
                }
            }
        } else if (key.endsWith('onMessage')) {
            input = Object.hasOwn(f, 'wire')
                ? f.wire
                : JSON.stringify(f.body ?? { ch: 'ticker', symbol: 'A', channel: 'c' });
            if (f.channelUnits)
                input = JSON.stringify({ ch: text(f.channelUnits), symbol: 'A', channel: text(f.channelUnits) });
            if (f.blob) {
                const bytes = f.badBlob ? Uint8Array.from([255, 0, 1, 2]) : pako.deflate(input);
                class TestBlob extends Blob {
                    async arrayBuffer() {
                        note('readBlob');
                        note('blobPending');
                        await Promise.resolve();
                        if (f.disposeAfterBlob) disposed = true;
                        note('blobReady');
                        return super.arrayBuffer();
                    }
                }
                input = new TestBlob([bytes]);
            }
            result = await vm.runInNewContext(methods[key], context).call(owner)({ data: input });
        } else {
            const ticker = { id: 'ticker' };
            let data = Object.hasOwn(f, 'data')
                ? f.data
                : {
                      la: '12.5',
                      o: '10',
                      b: '2.5',
                      q: '25',
                      mp: '12',
                      ip: '11.5',
                      fr: '-0.01',
                      ft: '123',
                      a: [['14', '2']],
                  };
            if (key.endsWith('handleDepthBookStreamEvent') && !Object.hasOwn(f, 'data'))
                data = { b: [['12.5', '2']], a: [['14', '2']] };
            if (f.fieldUnits)
                data = {
                    ...data,
                    [f.fieldUnits.key]:
                        key.endsWith('handleDepthBookStreamEvent') && f.fieldUnits.key === 'b'
                            ? [[text(f.fieldUnits.units)]]
                            : text(f.fieldUnits.units),
                };
            if (f.symbolValue) data = { ...data, la: Symbol('input') };
            const dataProxy =
                data && typeof data === 'object'
                    ? new Proxy(data, {
                          get: (d, k) => {
                              note('property', String(k));
                              return d[k];
                          },
                      })
                    : data;
            const event = {
                get symbol() {
                    note('symbol');
                    return 'A';
                },
                get data() {
                    note('data');
                    return dataProxy;
                },
            };
            owner.parent.store = {
                tickersBySymbol: {
                    get A() {
                        note('ticker');
                        return f.noTicker ? undefined : ticker;
                    },
                },
            };
            owner.store = {
                updateTicker: (t, patch) => {
                    assert.equal(t, ticker);
                    note('update', patch);
                },
            };
            result = vm.runInNewContext(methods[key], context).call(owner)(event);
        }
    } catch (e) {
        error = obs(e);
    }
    rows.push({
        key,
        fixture: f,
        events,
        leaves,
        parsed: obs(parsed),
        parseError: parseError ?? null,
        input: obs(input),
        decoded: obs(decoded),
        readError: readError ?? null,
        topicEntries: topicEntries ?? null,
        lookupKey: obs(lookupKey),
        count: obs(count),
        timer: obs(timer),
        error: error ?? null,
    });
}
(async () => {
    for (const scope of ['public', 'futures'])
        for (const f of [
            {},
            { disposed: true },
            { secondary: true },
            { flipSecondary: true },
            { timerBits: '3ff0000000000000' },
            { timerBits: '8000000000000000' },
            { timerBits: '7ff8000000000000' },
            { timerReads: [1, 2] },
            { disposeAtPing: true },
            { repeat: true, disposeBeforeRepeat: true },
            { fail: 'reset' },
            { fail: 'ping' },
            { fail: 'schedule' },
        ])
            await run(scope + '.onOpen', f);
    for (const scope of ['public', 'futures'])
        for (const f of [
            {},
            { disposed: true },
            { wire: 'invalid' },
            { wire: 'null' },
            { body: {} },
            { body: { ch: 'price', channel: 'price' } },
            { body: { ch: 'depth_book1', channel: 'depth_book1' } },
            { body: { ch: 'unknown', channel: 'unknown' } },
            { body: { ch: 7, channel: 7 } },
            { body: { ch: 'market_kline_1min', symbol: 'A', channel: 'market_kline_1min' } },
            { body: { ch: 'market_kline_1min', symbol: 'A' }, edit: 'append' },
            { body: { ch: 'market_kline_1min', symbol: 'A' }, edit: 'delete' },
            { body: { ch: 'market_kline_1min', symbol: 'A' }, throwHandler: true },
            { body: { ch: 'market_kline_1min', symbol: 'A' }, noHandlers: true },
            { channelUnits: [0xd800, 65, 0xdc00] },
            { countBits: '4340000000000000' },
            { countBits: '7ff0000000000000' },
            { countBits: '7ff8000000000000' },
            { fail: 'track' },
            { fail: 'setCount' },
            { fail: 'stats' },
            { fail: 'stream' },
            { fail: 'parse' },
        ])
            await run(scope + '.onMessage', f);
    for (const f of [
        { blob: true },
        { blob: true, badBlob: true },
        { blob: true, disposeAfterBlob: true },
        { blob: true, fail: 'log', badBlob: true },
    ])
        await run('futures.onMessage', f);
    for (const method of ['handleTickerStreamEvent', 'handlePriceStreamEvent', 'handleDepthBookStreamEvent'])
        for (const f of [
            {},
            { noTicker: true },
            { data: null },
            { data: {} },
            { data: { la: '-0', o: '0', b: [['-0']], a: [['bad']], mp: '-0', ip: 'Infinity', fr: 'NaN', ft: '1e309' } },
            { fieldUnits: { key: 'la', units: [49, 46, 53, 0xd800] } },
            { symbolValue: true },
            { fail: 'ticker' },
            { fail: 'data' },
            { fail: 'float' },
            { fail: 'update' },
        ])
            await run('public.' + method, f);
    for (const method of ['handleTickerStreamEvent', 'handlePriceStreamEvent', 'handleDepthBookStreamEvent']) {
        const key = method === 'handleTickerStreamEvent' ? 'la' : method === 'handlePriceStreamEvent' ? 'mp' : 'b';
        for (const units of [
            [0xd800, 49],
            [49, 101, 0xd800, 50],
            [0xfeff, 0x2028, 49, 46, 53, 0xd800],
            [49, 0xd83d, 0xde00, 50],
            [45, 48, 0xdc00],
        ])
            await run('public.' + method, { fieldUnits: { key, units } });
    }
    assert.equal(rows.length, 124);
    process.stdout.write(JSON.stringify({ rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
