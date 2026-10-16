const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts');
const source = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
const member = source.statements.find(ts.isClassDeclaration).members.find(m => m.name?.getText(source) === 'sendChannelOp');
if (!member?.initializer) throw Error('missing live sendChannelOp');
let expression = member.initializer.getText(source);
if (process.env.KUCOIN_BATCH_MUTATION === 'boundary') {
  if (!expression.includes('i + 100')) throw Error('missing mutation target');
  expression = expression.replaceAll('i + 100', 'i + 99');
}
const code = ts.transpileModule('module.exports=' + expression, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
process.stdout.write(JSON.stringify(cases.map(c => {
  const frames = []; let clockCalls = 0;
  const context = {send(frame) { frames.push(frame); if (frames.length === c.failAt) throw Error('send refused'); }};
  const module = {exports: {}};
  new Function('module', 'Date', code).call(context, module, {now() {return 1700000000000 + clockCalls++;}});
  let error = null;
  try { module.exports(c.subscribe ? 'subscribe' : 'unsubscribe', c.channels, c.symbols); }
  catch (e) { error = e.message; }
  return {frames, clockCalls, error};
})));
