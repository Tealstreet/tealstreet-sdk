const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = tree.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'ApexOmniExchange');
const member = owner.members.find(node => node.name?.getText(tree) === 'validateAccount');
assert(member?.initializer, 'actual validateAccount source initializer');
const code = ts.transpileModule(`const validate = (function() { return ${member.initializer.getText(tree)}; }).call(receiver); validate;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const messages = [17, -17.5, true, { code: 42, detail: { why: 'bad key' } }, { message: 17 }, {}, [], ['bad key', 17], 'bad key'];
(async () => {
  const rows = [];
  for (const message of messages) {
    const emitted = [];
    const receiver = {
      xhr: { get: async endpoint => {
        assert.equal(endpoint, '/v3/user');
        throw { response: { data: { msg: message } }, message: 'transport cause' };
      } },
      emitAndLogError: value => emitted.push(value),
    };
    const validate = vm.runInNewContext(code, { ENDPOINTS: { USER: '/v3/user' }, receiver: receiver,
      initSdk: () => { throw new Error('unexpected SDK initialization after user refusal'); } });
    const result = await validate.call(receiver);
    assert.deepEqual(result.error, message);
    assert.deepEqual(emitted, [message]);
    rows.push({ message, result, emitted });
  }
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
