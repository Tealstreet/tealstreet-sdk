const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let sourceText = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_CHANGE_STP_CONTROL === 'accept-invalid') {
  const original = 'if (!isAsterDEXStpMode(stpMode))';
  if (!sourceText.includes(original)) throw Error('changeStpMode source control target disappeared');
  sourceText = sourceText.replace(original, 'if (false)');
}
const ast = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const member = name => {
  const node = cls.members.find(n => n.name?.getText(ast) === name);
  if (!node) throw Error(`Missing ${name}`);
  return printer.printNode(ts.EmitHint.Unspecified, node, ast);
};
const source = `class Oracle {
  apiVersion = 'v3';
  emits = [];
  emitAndLogError(message) { this.emits.push(message); }
  ${member('getFapiPath')}
  ${member('assertV3Feature')}
  ${member('fetchStpMode')}
  ${member('changeStpMode')}
}
export { Oracle };`;
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'getAsterDEXFapiPath', 'isAsterDEXStpMode', compiled)(loaded.exports, values.getAsterDEXFapiPath, values.isAsterDEXStpMode);
const { Oracle } = loaded.exports;
if (process.env.ASTERDEX_STP_CONTROL === 'drop-first-mode') {
  values.ASTERDEX_STP_MODES[0] = 'EXPIRE_TAKER_CONTROL';
}
const inputs = [undefined, null, {}, { stpMode: undefined }, { stpMode: null },
  ...['EXPIRE_TAKER', 'EXPIRE_MAKER', 'EXPIRE_BOTH', 'expire_taker', '', 0, false, {}, []]
    .map(stpMode => ({ stpMode }))];
const result = async fn => {
  try { return { value: await fn() }; }
  catch (error) { return { error: { name: error.name, message: error.message } }; }
};
(async () => {
  const output = [];
  for (const data of inputs) {
    const oracle = new Oracle();
    let requestPath;
    oracle.primaryAPI = { get: async url => { requestPath = url; return { data }; } };
    output.push({ data: data === undefined ? { undefined: true } : data,
      ...await result(() => oracle.fetchStpMode()), requestPath, emits: oracle.emits });
  }
  const changeInputs = [undefined, null, 'EXPIRE_TAKER', 'EXPIRE_MAKER', 'EXPIRE_BOTH',
    'expire_taker', '', 0, false, {}, { toString: 'blocked' }, []];
  for (const stpMode of changeInputs) {
    const oracle = new Oracle();
    let requestPath;
    let requestBody;
    oracle.primaryAPI = { post: async (path, body) => { requestPath = path; requestBody = body; } };
    output.push({ operation: 'changeStpMode', input: stpMode,
      ...await result(() => oracle.changeStpMode(stpMode)), requestPath, requestBody, emits: oracle.emits });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { console.error(error); process.exitCode = 1; });
