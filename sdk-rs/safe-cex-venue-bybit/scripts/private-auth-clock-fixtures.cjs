const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function members(relative, owner, names) {
  const file = path.join(root, relative);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === owner);
  assert(cls, owner);
  return names.map(name => {
    const selected = cls.members.filter(node => node.name?.getText(ast) === name);
    assert.equal(selected.length, 1, owner + '.' + name);
    return selected[0].getText(ast);
  }).join('\n');
}
const clock = members('packages/safe-cex/src/utils/virtualClock.ts', 'VirtualClock',
  ['server', 'client', 'offsetMs', 'getCurrentTimeMs', 'syncBybitClientServerTime']);
const auth = members('packages/safe-cex/src/exchanges/bybit/bybitinverse.ws-private.ts', 'BybitInversePrivateWebsocket', ['auth']);
async function main() {
  const profiles = [];
  for (const offset of [0, 300000, -300000]) {
    // Select the production Node branch; retain its real Date.now and source clock body.
    const context = vm.createContext({ isNodeOrBunRuntime: () => true, nanoid: () => 'native-clock-id' });
    vm.runInContext(ts.transpileModule(`class Clock { ${clock} } globalThis.virtualClock = new Clock(); class Subject { ${auth} } globalThis.api = new Subject();`, {
      compilerOptions: { target: ts.ScriptTarget.ES2022 },
    }).outputText, context);
    context.virtualClock.offsetMs = offset;
    await context.virtualClock.syncBybitClientServerTime();
    assert.equal(context.virtualClock.offsetMs, 0, 'native reconnect clears the previous offset');
    let request;
    context.api.parent = { options: { key: '', remoteSigner: { accountName: 'inverse-account', sign(value) {
      assert.equal(request, undefined, 'one original signer invocation');
      request = value;
      return new Promise(() => {});
    }}}};
    context.api.sendWithLog = () => assert.fail('held signer cannot send auth');
    const before = Date.now();
    void context.api.auth();
    const after = Date.now();
    assert(request, 'the actual auth prefix invokes the original signer');
    assert.equal(request.type, 'ws-auth');
    assert.equal(request.exchangeName, 'bybit');
    assert.equal(request.accountName, 'inverse-account');
    const expires = request.wsAuth.extraData.expires;
    assert(Number.isFinite(expires), 'the production clock plus fixed window stays finite');
    assert(expires >= before + 10000 && expires <= after + 10000, 'clock sample and fixed window');
    assert.equal(request.wsAuth.toSign, `GET/realtime${expires}`);
    profiles.push({offset, finite: true, window: 10000});
  }
  console.log(JSON.stringify(profiles));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
