const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.ws-private.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration),
    member = cls.members.filter((n) => n.name?.getText(ast) === 'sendLogin');
assert.equal(member.length, 1);
const code = ts.transpileModule('(function(){return (' + member[0].initializer.getText(ast) + ');})', {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText;
const uf = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.utils.ts'),
    ua = ts.createSourceFile(uf, fs.readFileSync(uf, 'utf8'), ts.ScriptTarget.Latest, true),
    sha = ua.statements.find((n) => ts.isFunctionDeclaration(n) && n.name?.text === 'sha256Hex');
assert(sha);
const sha256Hex = vm.runInNewContext(
    ts.transpileModule('(' + sha.getText(ua).replace(/^export /, '') + ')', {
        compilerOptions: { target: ts.ScriptTarget.ES2022 },
    }).outputText,
    {
        ...require('@noble/hashes/sha2.js'),
        ...require('@noble/hashes/utils.js'),
    },
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
const rows = [];
(async () => {
    for (const f of [
        {},
        { key: '\uFEFF K \u2029', secret: '\u00a0S\uFEFF' },
        { key: 'é😀', secret: '秘' },
        { timeBits: '8000000000000000' },
        { timeBits: '7ff8000000000000' },
        { remote: true },
        { remote: true, result: { error: 'timeout' } },
        { remote: true, result: {} },
        { remote: true, result: { error: false, wsAuthSignature: '' } },
        { remote: true, result: { error: 0, wsAuthSignature: 0 } },
        { remote: true, result: { wsAuthSignature: true } },
        { remote: true, result: { wsAuthSignature: { x: 'sig' } } },
        { remote: true, result: null },
        { remote: true, changeAtSync: true },
        { fail: 'clock' },
        { fail: 'nonce' },
        { fail: 'key' },
        { fail: 'sync' },
        { fail: 'sleep' },
        { fail: 'secret' },
        { remote: true, fail: 'id' },
        { remote: true, fail: 'account' },
        { remote: true, fail: 'sign' },
        { remote: true, result: { error: 'offline' }, fail: 'report' },
        { fail: 'send' },
    ]) {
        const events = [];
        let key = f.key ?? ' K ',
            secret = f.secret ?? ' S ',
            remote = f.remote;
        const note = (name, ...args) => {
            events.push([name, ...args.map(obs)]);
            if (f.fail === name) throw new Error(name + ' failed');
        };
        const signer = {
            get accountName() {
                note('account');
                return 'account';
            },
            sign: async (request) => {
                note('sign', request);
                return Object.hasOwn(f, 'result') ? f.result : { wsAuthSignature: 'REMOTE' };
            },
        };
        const options = {
            get key() {
                note('key');
                return key;
            },
            get secret() {
                note('secret');
                return secret;
            },
            get remoteSigner() {
                note('remote');
                return remote ? signer : undefined;
            },
        };
        const context = {
            sha256Hex,
            nanoid: () => {
                note('id');
                return 'ID';
            },
            uuid: () => {
                note('nonce');
                return 'N';
            },
            virtualClock: {
                getCurrentTime: () => {
                    note('clock');
                    return Buffer.from(f.timeBits ?? '427950afe2400000', 'hex').readDoubleBE();
                },
                syncBybitClientServerTime: async () => {
                    note('sync');
                    if (f.changeAtSync) {
                        key = 'NEW';
                        secret = 'CHANGED';
                        remote = true;
                    }
                    await Promise.resolve();
                },
            },
            sleep: async (delay) => {
                note('sleep', delay);
                await Promise.resolve();
            },
            JSON: {
                stringify: (frame) => {
                    note('frame', frame);
                    return JSON.stringify(frame);
                },
            },
        };
        const owner = {
            parent: { options, emitAndLogError: (message) => note('report', message) },
            send: (wire) => note('send', wire),
        };
        let error;
        try {
            await vm.runInNewContext(code, context).call(owner)();
        } catch (e) {
            error = { name: e.name, message: e.message };
        }
        rows.push({ fixture: f, events, error: error ?? null });
    }
    assert.equal(rows.length, 25);
    process.stdout.write(JSON.stringify({ rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
