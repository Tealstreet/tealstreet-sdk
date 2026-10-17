const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const apiFile = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.api.ts');
const ast = ts.createSourceFile(apiFile, fs.readFileSync(apiFile, 'utf8'), ts.ScriptTarget.Latest, true);
const node = ast.statements
    .filter(ts.isVariableStatement)
    .flatMap((n) => [...n.declarationList.declarations])
    .filter((n) => n.name.getText(ast) === 'createAPI');
assert.equal(node.length, 1);
const code = ts.transpileModule('(' + node[0].initializer.getText(ast) + ')', {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText;
const { BASE_URL } = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const keyFile = path.join(root, 'packages/safe-cex/src/utils/get-key.ts');
const keyModule = { exports: {} };
vm.runInThisContext(
    '(function(require,module,exports){' +
        ts.transpileModule(fs.readFileSync(keyFile, 'utf8'), {
            compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true },
        }).outputText +
        '\n})',
)(
    (id) => {
        assert(['lodash/camelCase', 'lodash/memoize', 'lodash/snakeCase'].includes(id), id);
        return require('' + id);
    },
    keyModule,
    keyModule.exports,
);
const { v } = keyModule.exports;
const { appendHttpFailure } = load(path.join(root, 'packages/safe-cex/src/utils/httpFailure.ts'));
const qs = require('qs');
const noble = require('@noble/hashes/sha2.js'),
    utils = require('@noble/hashes/utils.js');
const utilFile = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.utils.ts');
const utilAst = ts.createSourceFile(utilFile, fs.readFileSync(utilFile, 'utf8'), ts.ScriptTarget.Latest, true);
const sha = utilAst.statements.find(ts.isFunctionDeclaration);
const sha256Hex = vm.runInNewContext(
    ts.transpileModule('(' + sha.getText(utilAst).replace(/^export /, '') + ')', {
        compilerOptions: { target: ts.ScriptTarget.ES2022 },
    }).outputText,
    { ...noble, ...utils },
);
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
function make(fixture) {
    const events = [];
    let request, response;
    const condition = () => true;
    const client = {
        interceptors: {
            request: {
                use: (fn) => {
                    events.push(['request']);
                    request = fn;
                },
            },
            response: {
                use: (fn) => {
                    events.push(['response']);
                    response = fn;
                },
            },
        },
    };
    const note = (name, ...args) => {
        events.push(obs([name, ...args]));
        if (fixture.fail === name) throw new Error(name + ' failed');
    };
    const context = {
        BASE_URL,
        v,
        appendHttpFailure: (message, response) => {
            note('report', message);
            return appendHttpFailure(message, response);
        },
        sha256Hex,
        uuid: () => {
            note('nonce');
            return fixture.nonce ?? 'N';
        },
        virtualClock: {
            getCurrentTime: () => {
                note('clock');
                return Buffer.from(fixture.timeBits ?? '427950afe2400000', 'hex').readDoubleBE();
            },
        },
        qs: {
            stringify: (params, opts) => {
                note('query', params, opts.arrayFormat, 'localeCompare');
                return qs.stringify(params, opts);
            },
        },
        AxiosHeaders: class {
            constructor(raw) {
                note('headers', raw);
                Object.assign(this, raw);
            }
        },
        safeCexAxiosCreate: (config, options, exchange) => {
            note('create', config);
            return client;
        },
        networkErrorOnRead: () => {
            note('condition');
            return condition;
        },
        retry: (c, opts) => {
            assert.equal(c, client);
            // networkErrorOnRead() now runs per evaluation, inside the condition.
            assert.equal(typeof opts.retryCondition, 'function');
            note('retry', opts.retries);
        },
        isBitunixSignatureRejection: function isBitunixSignatureRejection() {},
        signatureRejectedResponse: () => {
            note('signature-condition');
            return () => false;
        },
        resendOnSignatureRejection: (c, response, predicate) => {
            assert.equal(c, client);
            note('resend', predicate.name);
            return fixture.resend ? 'RESENT' : undefined;
        },
        installTransportAdapter: (c, o, name) => {
            assert.equal(c, client);
            note('transport', name);
        },
    };
    const options = fixture.options ?? { key: 'K', secret: 'S' };
    const exchange = { name: 'bitunix', emitAndLogError: (msg) => note('emit', msg) };
    let result, error;
    try {
        result = vm.runInNewContext(code, context)(options, exchange);
    } catch (e) {
        error = { name: e.name, message: e.message };
    }
    return { events, options, client, request, response, note, result, error };
}
// Additional production-client observations; the default corpus is unchanged.
if (process.argv[2] === '--native-query') {
    const samples = JSON.parse(process.argv[3]);
    const results = samples.map(sample => {
        const run = make({options:{key:'fixture-key',secret:'fixture-secret'}, nonce:sample.nonce,timeBits:sample.timeBits});
        const result = run.request({method:'GET',url:'/api/v1/futures/account',params:sample.params});
        return Object.fromEntries(Object.entries(result.headers).map(([key,value])=>[key,String(value)]));
    });
    fs.writeSync(1, JSON.stringify(results));
    process.exit(0);
}
const factory = [];
for (const testnet of [false, true])
    for (const publicDataOnly of [false, true]) {
        const f = { options: { testnet, publicDataOnly } };
        const run = make(f);
        factory.push({ fixture: f, events: run.events, identity: run.result === run.client, error: run.error ?? null });
    }
for (const fail of ['create', 'condition', 'retry', 'transport']) {
    const run = make({ fail });
    factory.push({ fixture: { fail }, events: run.events, identity: run.result === run.client, error: run.error ?? null });
}
const requests = [];
const base = {
    url: '/api/v1/futures/trade/order',
    params: { z: 1, a: 'x y', arr: [1, 2] },
    data: { qty: '2.5', price: '12.25' },
    headers: { old: 'keep', sign: 'stale' },
};
for (const fixture of [
    {},
    { config: { ...base, url: '/public/x' } },
    { config: { ...base, url: '/PUBLIC/x' } },
    { config: { ...base, params: null } },
    { config: { ...base, data: false } },
    { config: { ...base, data: [] } },
    { options: {} },
    { options: { key: null, secret: null } },
    { options: { key: 'é😀', secret: '秘' } },
    { timeBits: '8000000000000000' },
    { timeBits: '7ff8000000000000' },
    { fail: 'query' },
    { fail: 'clock' },
    { fail: 'nonce' },
    { fail: 'headers' },
    { config: { ...base, headers: null } },
    { config: { ...base, data: { 2: 'b', 1: 'a', str: 'x' } } },
    { config: { ...base, data: '{"qty":"2.5","price":"12.25"}' } },
]) {
    const run = make(fixture);
    run.events.length = 0;
    const config = fixture.config ?? base;
    let result, error;
    try {
        result = run.request(config);
    } catch (e) {
        error = { name: e.name, message: e.message };
    }
    const queryResult = config.params
        ? decodeURIComponent(qs.stringify(config.params, { arrayFormat: 'repeat', sort: (a, b) => a.localeCompare(b) }))
        : null;
    requests.push({
        fixture: JSON.stringify({ ...fixture, config }),
        queryResult,
        events: run.events,
        error: error ?? null,
        identity: result === config,
    });
}
const responses = [];
(async () => {
    for (const fixture of [
        {},
        { data: { data: [], msg: 'error' } },
        { data: { data: 0, msg: 'bad' } },
        { data: { msg: 17 } },
        { data: null },
        { data: { msg: 'REQUEST TOO FREQUENTLY' }, rateFlag: true },
        { data: { msg: 'different error' }, rateFlag: true },
        { errorFlag: true },
        { errorFlag: 'true' },
        { headerValue: '1' },
        { headerValue: 'TrUe' },
        { rateValue: 'true', data: { msg: 'request too frequently' } },
        { headerValues: ['0', 'true'] },
        { headerValue: { toString: null } },
        { fail: 'header' },
        { fail: 'emit' },
        { status: 429 },
        { resend: true },
        { twice: true },
    ]) {
        const run = make({ fail: fixture.fail === 'emit' ? 'emit' : undefined, resend: fixture.resend });
        run.events.length = 0;
        let reads = 0;
        const config = {
            __suppressExchangeError: fixture.errorFlag,
            __suppressExchangeRateLimitError: fixture.rateFlag,
            headers: {
                get: (name) => {
                    run.note('header', name);
                    if (fixture.fail === 'header') throw new Error('header failed');
                    return name === 'x-tealstreet-suppress-exchange-error'
                        ? fixture.headerValues
                            ? fixture.headerValues[reads++]
                            : fixture.headerValue
                        : fixture.rateValue;
                },
            },
        };
        const response = {
            data: Object.hasOwn(fixture, 'data') ? fixture.data : { msg: 'bad' },
            config,
            status: fixture.status,
        };
        let result, error;
        try {
            result = await run.response(response);
            // An axios-retry resend reaches the interceptor again with the same response.
            if (fixture.twice) result = await run.response(response);
        } catch (e) {
            error = { name: e.name, message: e.message };
        }
        responses.push({
            fixture: JSON.stringify(fixture),
            events: run.events,
            error: error ?? null,
            identity: result === response,
            resent: result === 'RESENT',
        });
    }
    const signatureCorpus = [{code:10007},{code:'10007'},{msg:'Sign Signature Error'},{code:1,msg:'sign signature error: bad'},{msg:['sign signature error']},{code:[10007]},{code:10006},{code:null,msg:null},{msg:'signature error'},'10007',[],null];
    const { isBitunixSignatureRejection } = (() => {
        const declaration = ast.statements
            .filter(ts.isVariableStatement)
            .flatMap((n) => [...n.declarationList.declarations])
            .find((n) => n.name.getText(ast) === 'isBitunixSignatureRejection');
        const fn = vm.runInNewContext(
            ts.transpileModule('(' + declaration.initializer.getText(ast) + ')', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText,
        );
        return { isBitunixSignatureRejection: fn };
    })();
    const signature = { corpus: signatureCorpus, results: signatureCorpus.map((data) => isBitunixSignatureRejection(data)) };
    assert.equal(factory.length, 8);
    assert.equal(requests.length, 18);
    assert.equal(responses.length, 19);
    process.stdout.write(JSON.stringify({ factory, requests, responses, signature }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
