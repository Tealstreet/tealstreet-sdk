'use strict';
const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const files = { public: 'bitunix.ws-public.ts', private: 'bitunix.ws-private.ts', futures: 'bitunix.ws-futures.ts' };
const asts = {};
for (const [kind, name] of Object.entries(files)) {
    const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix', name);
    asts[kind] = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
}
function declaration(name) {
    const nodes = asts.public.statements
        .flatMap((n) => (ts.isVariableStatement(n) ? [...n.declarationList.declarations] : []))
        .filter((n) => n.name.getText(asts.public) === name);
    assert.equal(nodes.length, 1, `WS declaration ${name}`);
    return vm.runInNewContext('(' + nodes[0].initializer.getText(asts.public) + ')');
}
const MAX = declaration('BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION'),
    INTERVAL = declaration('INTERVAL');
const bits = (n) => {
    const b = Buffer.alloc(8);
    b.writeDoubleBE(n);
    return Number.isNaN(n) ? '7ff8000000000000' : b.toString('hex');
};
const fromBits = (s) => Buffer.from(s, 'hex').readDoubleBE();
function obs(v) {
    if (v === undefined) return { type: 'undefined' };
    if (v === null) return { type: 'null' };
    if (typeof v === 'number') return { type: 'number', bits: bits(v) };
    if (typeof v === 'boolean') return { type: 'boolean', value: v };
    if (typeof v === 'string')
        return { type: 'string', units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
    if (Array.isArray(v)) return { type: 'array', items: v.map(obs) };
    return { type: 'object', entries: Object.entries(v).map(([k, v]) => [k, obs(v)]) };
}
const arrows = {};
const methods = {
    public: ['ping', 'clearSubscriptionQueue', 'drainSubscriptionQueue', 'dispose'],
    private: [
        'ping',
        'throttledFetchBalance',
        'throttledFetchOrders',
        'throttledFetchPositions',
        'fetchThrottledBalance',
        'fetchThrottledOrders',
        'fetchThrottledPositions',
    ],
    futures: ['ping'],
};
for (const [kind, names] of Object.entries(methods)) {
    const owners = asts[kind].statements.filter(ts.isClassDeclaration);
    assert.equal(owners.length, 1, `WS owner ${kind}`);
    for (const name of names) {
        const n = owners[0].members.filter((n) => n.name?.getText(asts[kind]) === name);
        assert.equal(n.length, 1, `WS method ${kind}.${name}`);
        arrows[kind + '.' + name] = ts.transpileModule(
            `(function(){return (${n[0].initializer.getText(asts[kind])});})`,
            { compilerOptions: { target: ts.ScriptTarget.ES2022 } },
        ).outputText;
    }
}
const publicPings = [
    {},
    { disposed: true },
    { sendFail: true },
    { performanceFail: true },
    { stringifyFail: true },
    { perfBits: bits(-0), timeBits: bits(NaN) },
    { clockFail: true },
    { pingAtFail: true },
];
const privatePings = [
    {},
    { disposed: true },
    { sendFail: true },
    { performanceFail: true },
    { pongFail: true },
    { trackFail: true },
    { pingAtFail: true },
];
const debounce = [
    {},
    { timer: 0 },
    { timer: 7 },
    { timer: 7, secondTimer: 0 },
    { timer: 7, scheduleFail: true },
    { timer: 7, clearFail: true },
    { setFail: true },
    { handlerFail: true },
];
const fetches = [
    {},
    { disposed: true },
    { disposeAfterFetch: true },
    { fetchFail: true },
    { updateFail: true },
    { loadedFail: true },
];
const clears = [
    { timer: 7 },
    {},
    { timer: 7, clearFail: true },
    { timer: 7, setFail: true },
    { timer: 7, replaceFail: true },
];
const drains = [
    { queue: [] },
    { disposed: true, queue: [[{ symbol: 'A', ch: 'ticker' }]] },
    { queue: [[{ symbol: 'A', ch: 'ticker' }]] },
    { queue: [[], [{ symbol: 'B', ch: 'price' }]] },
    { queue: [[{ symbol: 'A', ch: 'ticker' }]], sendFail: true },
    { queue: [[{ symbol: 'A', ch: 'ticker' }]], stringifyFail: true },
    { queue: [[{ symbol: 'A', ch: 'ticker' }]], scheduleFail: true },
    { queue: [[{ symbol: 'A', ch: 'ticker' }]], setFail: true },
];
const disposals = [{}, { offFail: true }, { clearFail: true }, { queueClearFail: true }];
async function run(key, fixture) {
    const events = [];
    const record = (...v) => events.push(v.map(obs));
    const err = (id) => Object.assign(new Error(id + ' failed'), { id });
    const state = {
        timer: fixture.timer,
        pingAt: 99,
        disposed: !!fixture.disposed,
        queue: fixture.queue ? structuredClone(fixture.queue) : [[{ symbol: 'OLD', ch: 'ticker' }]],
        loaded: { balance: false, positions: false, orders: false, other: 17 },
    };
    const oldQueue = state.queue;
    let reads = 0;
    let owner;
    const context = {
        performance: {
            now: () => {
                record('performance');
                if (fixture.performanceFail) throw err('performance');
                return fromBits(fixture.perfBits ?? bits(10.5));
            },
        },
        virtualClock: {
            getCurrentTimeMs: () => {
                record('clock');
                if (fixture.clockFail) throw err('clock');
                return fromBits(fixture.timeBits ?? bits(1740000000000));
            },
        },
        BITUNIX_SUBSCRIBE_INTERVAL_MS: declaration('BITUNIX_SUBSCRIBE_INTERVAL_MS'),
        JSON: {
            parse: JSON.parse,
            stringify: (value) => {
                record('frame', value);
                if (fixture.stringifyFail) throw err('stringify');
                return JSON.stringify(value);
            },
        },
        clearTimeout: (timer) => {
            record('clear', timer);
            if (fixture.clearFail) throw err('clear');
        },
        setTimeout: (handler, delay) => {
            record('schedule', handler.handlerName, delay);
            if (fixture.scheduleFail) throw err('schedule');
            return 500;
        },
    };
    owner = {
        isDisposed: state.disposed,
        sendWithLog: (wire) => {
            record('sendLog', wire);
            if (fixture.sendFail) throw err('send');
        },
        send: (wire) => {
            record('send', wire);
            if (fixture.sendFail) throw err('send');
        },
        handlePongEvent: () => {
            record('pong');
            if (fixture.pongFail) throw err('pong');
        },
        trackLastReceived: () => {
            record('track');
            if (fixture.trackFail) throw err('track');
        },
        clearSubscriptionQueue: () => {
            record('clearQueue');
            if (fixture.queueClearFail) throw err('queue');
        },
        parent: {
            emitter: {
                off: (topic, handler) => {
                    record('off', topic, handler.handlerName);
                    if (fixture.offFail) throw err('off');
                },
            },
            store: {
                update: (patch) => {
                    record('update', patch);
                    if (fixture.updateFail) throw err('update');
                },
            },
        },
    };
    owner.onMarketsChanged = Object.assign(() => {}, { handlerName: 'onMarketsChanged' });
    owner.drainSubscriptionQueue = Object.assign(() => {}, { handlerName: 'drainSubscriptionQueue' });
    Object.defineProperty(owner, 'pingAt', {
        set: (value) => {
            record('pingAt', value);
            if (fixture.pingAtFail) throw err('pingAt');
            state.pingAt = value;
        },
    });
    const timerField =
        key.includes('throttledFetch') || key.includes('fetchThrottled')
            ? key.endsWith('Balance')
                ? 'balanceThrottleTimeout'
                : key.endsWith('Orders')
                  ? 'ordersThrottleTimeout'
                  : 'positionsThrottleTimeout'
            : 'subscriptionTimeoutId';
    Object.defineProperty(owner, timerField, {
        get: () => {
            let value = reads++ === 1 && Object.hasOwn(fixture, 'secondTimer') ? fixture.secondTimer : state.timer;
            record('getTimer', timerField, value);
            return value;
        },
        set: (value) => {
            record('setTimer', timerField, value);
            if (fixture.setFail) throw err('set');
            state.timer = value;
        },
    });
    Object.defineProperty(owner, 'pingLoopTimeoutId', {
        get: () => {
            record('getPingTimer');
            return fixture.timer;
        },
    });
    Object.defineProperty(owner, 'subscriptionQueue', {
        get: () => state.queue,
        set: (value) => {
            record('replaceQueue', value);
            if (fixture.replaceFail) throw err('replace');
            state.queue = value;
        },
    });
    if (key.includes('.throttledFetch')) {
        const handlerName = key.replace('.throttledFetch', '.fetchThrottled').split('.')[1];
        Object.defineProperty(owner, handlerName, {
            get: () => {
                record('handler', handlerName);
                if (fixture.handlerFail) throw err('handler');
                return Object.assign(() => {}, { handlerName });
            },
        });
    }
    if (key.includes('.fetchThrottled')) {
        const field = key.endsWith('Balance') ? 'balance' : key.endsWith('Orders') ? 'orders' : 'positions';
        Object.defineProperty(owner.parent.store, 'loaded', {
            get: () => {
                record('loaded');
                if (fixture.loadedFail) throw err('loaded');
                return state.loaded;
            },
        });
        owner.parent['fetch' + field[0].toUpperCase() + field.slice(1)] = async () => {
            record('fetch', field);
            if (fixture.fetchFail) throw err('fetch');
            if (fixture.disposeAfterFetch) {
                owner.isDisposed = true;
                state.disposed = true;
                state.loaded.after = 42;
            }
            return field === 'balance'
                ? { balance: { free: 3 }, positions: [{ id: 'P' }] }
                : field === 'orders'
                  ? [{ id: 'O' }]
                  : [{ id: 'P' }];
        };
    }
    const factory = key === 'public.dispose' ? null : vm.runInNewContext(arrows[key], context);
    let error;
    // super.dispose requires a real home object, so evaluate that one source body
    // on an actual subclass rather than converting its arrow to a free expression.
    try {
        if (key === 'public.dispose') {
            const cls = asts.public.statements.find(ts.isClassDeclaration);
            const method = cls.members.find((n) => n.name?.getText(asts.public) === 'dispose');
            const code = ts.transpileModule(`class Port extends Base {${method.getText(asts.public)}}; Port`, {
                compilerOptions: { target: ts.ScriptTarget.ES2022 },
            }).outputText;
            const Base = class {
                dispose() {
                    record('baseDispose');
                }
            };
            const C = vm.runInNewContext(code, { ...context, Base });
            const instance = new C();
            Object.defineProperties(instance, Object.getOwnPropertyDescriptors(owner));
            owner = instance;
            owner.dispose();
        } else {
            owner[key.split('.')[1]] = Object.assign(factory.call(owner), { handlerName: key.split('.')[1] });
            await owner[key.split('.')[1]]();
        }
    } catch (e) {
        error = { name: e.name, message: e.message, id: e.id ?? null };
    }
    return {
        key,
        fixture,
        events,
        error: error ?? null,
        timer: obs(state.timer),
        pingAt: obs(state.pingAt),
        queue: obs(state.queue),
        oldQueue: obs(oldQueue),
        queueIdentity: state.queue === oldQueue,
        disposed: state.disposed,
    };
}
(async () => {
    const rows = [];
    for (const kind of ['public', 'futures'])
        for (const fixture of publicPings) rows.push(await run(kind + '.ping', fixture));
    for (const fixture of privatePings) rows.push(await run('private.ping', fixture));
    for (const name of ['throttledFetchBalance', 'throttledFetchOrders', 'throttledFetchPositions'])
        for (const fixture of debounce) rows.push(await run('private.' + name, fixture));
    for (const name of ['fetchThrottledBalance', 'fetchThrottledOrders', 'fetchThrottledPositions'])
        for (const fixture of fetches) rows.push(await run('private.' + name, fixture));
    for (const fixture of clears) rows.push(await run('public.clearSubscriptionQueue', fixture));
    for (const fixture of drains) rows.push(await run('public.drainSubscriptionQueue', fixture));
    for (const fixture of disposals) rows.push(await run('public.dispose', fixture));
    assert.equal(rows.length, 82, 'WS independent lifecycle source total');
    assert.equal(Object.keys(INTERVAL).length, 12, 'WS independent interval source total');
    process.stdout.write(JSON.stringify({ max: obs(MAX), interval: obs(INTERVAL), rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
