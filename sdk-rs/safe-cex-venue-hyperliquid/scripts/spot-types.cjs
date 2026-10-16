// Actual spot exports and strict TypeScript assignability, not a copied schema.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const directory = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid');
const spotPath = path.join(directory, 'hyperliquidspot.types.ts');
const spotText = fs.readFileSync(spotPath, 'utf8');
const parse = (name, text) => ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true);
const spot = parse(spotPath, spotText);
const core = parse('hyperliquid.types.ts', fs.readFileSync(path.join(directory, 'hyperliquid.types.ts'), 'utf8'));
const types = ['Tif', 'Tpsl', 'LimitOrderType', 'TriggerOrderType', 'Grouping', 'OrderType'];
const printer = ts.createPrinter({removeComments:true});
for (const name of types) {
  const declaration = file => file.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === name);
  const a = declaration(spot), b = declaration(core);
  assert(a && b, `SPOT source declaration ${name}`);
  assert.equal(printer.printNode(ts.EmitHint.Unspecified, a.type, spot), printer.printNode(ts.EmitHint.Unspecified, b.type, core), `SPOT closed transitive declaration ${name}`);
}
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(spotText, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports:exportsObject});
const constantNames = ['BASE_URLS','WSS_URLS','ENDPOINTS','GRAFANA_ENDPOINTS_EVERY','GRAFANA_ENDPOINTS_PERIODIC','INFO_TYPES','EXCHANGE_TYPES','WEBSOCKET'];
assert.deepEqual(Object.keys(exportsObject).sort(), [...constantNames].sort(), 'SPOT actual export root floor');
const constants = Object.fromEntries(constantNames.map(name => [name, Array.isArray(exportsObject[name]) ? exportsObject[name] : Object.entries(exportsObject[name])]));
const cases = [];
function add(type, inputs, acceptedCount) {
  inputs.forEach((input, index) => cases.push({type,input,expectedAccepted:index < acceptedCount}));
}
add('Tif', ['"Alo"','"Ioc"','"Gtc"','""','null','42','{"Alo":null}'], 3);
add('Tpsl', ['"tp"','"sl"','""','null','true','{"tp":null}'], 2);
add('Grouping', ['"na"','"normalTpsl"','"positionTpsl"','""','null','17','{"na":null}'], 3);
add('LimitOrderType', ['{"tif":"Alo"}','{"tif":"Ioc"}','{"tif":"Gtc"}','{}','{"tif":null}','{"tif":""}','["Alo"]'], 3);
add('TriggerOrderType', [
  '{"triggerPx":91.25,"isMarket":true,"tpsl":"tp"}',
  '{"triggerPx":-0,"isMarket":false,"tpsl":"sl"}',
  '{"triggerPx":1e400,"isMarket":false,"tpsl":"tp"}',
  '{"triggerPx":-1e400,"isMarket":true,"tpsl":"sl"}',
  '{"triggerPx":null,"isMarket":true,"tpsl":"tp"}',
  '{"triggerPx":17.5,"isMarket":0,"tpsl":"tp"}',
  '{"triggerPx":17.5,"tpsl":"tp"}',
  '{"triggerPx":17.5,"isMarket":true,"tpsl":"bad"}',
  '[17.5,true,"tp"]',
], 4);
add('OrderType', [
  '{}', '{"limit":{"tif":"Alo"}}',
  '{"trigger":{"triggerPx":91.25,"isMarket":true,"tpsl":"tp"}}',
  '{"limit":{"tif":"Ioc"},"trigger":{"triggerPx":17.5,"isMarket":false,"tpsl":"sl"}}',
  '{"limit":{"tif":"Gtc"},"trigger":{"triggerPx":-0,"isMarket":true,"tpsl":"sl"}}',
  'null','{"limit":null}','{"trigger":null}','[]','{"limit":{"tif":""}}',
  '{"trigger":{"triggerPx":"17.5","isMarket":true,"tpsl":"sl"}}',
], 5);
assert.equal(cases.length,47,'SPOT independent input floor');
const probePath = path.join(directory, '__spot_types_probe__.ts');
const probe = `import * as Spot from './hyperliquidspot.types';\n` + cases.map((row, index) => `const case_${index}: Spot.${row.type} = ${row.input};`).join('\n');
const options = {strict:true,noEmit:true,target:ts.ScriptTarget.ES2022,skipLibCheck:true};
const host = ts.createCompilerHost(options);
const originalRead = host.readFile.bind(host), originalExists = host.fileExists.bind(host);
host.readFile = name => name === probePath ? probe : originalRead(name);
host.fileExists = name => name === probePath || originalExists(name);
const program = ts.createProgram([probePath], options, host);
const file = program.getSourceFile(probePath);
assert(file, 'SPOT compiler probe input');
const rejected = new Set();
for (const diagnostic of program.getSemanticDiagnostics(file)) {
  assert(diagnostic.start !== undefined, 'SPOT located checker error');
  const line = file.getLineAndCharacterOfPosition(diagnostic.start).line;
  assert(line > 0 && line <= cases.length, ts.flattenDiagnosticMessageText(diagnostic.messageText,'\n'));
  assert([2322,2741,2739,2559].includes(diagnostic.code), `SPOT unexpected compiler diagnostic ${diagnostic.code}`);
  rejected.add(line-1);
}
const bits = number => {const buffer=Buffer.alloc(8);buffer.writeDoubleBE(number);return buffer.toString('hex');};
const trigger = value => ({priceBits:bits(value.triggerPx),isMarket:value.isMarket,tpsl:value.tpsl});
for (const [index,row] of cases.entries()) {
  row.accepted = !rejected.has(index);
  assert.equal(row.accepted,row.expectedAccepted,`SPOT actual strict checker ${index} ${row.type} ${row.input}`);
  if (!row.accepted) continue;
  const value = JSON.parse(row.input);
  row.native = row.type === 'TriggerOrderType' ? trigger(value) : row.type === 'OrderType' ? {limit:value.limit?.tif ?? null,trigger:value.trigger ? trigger(value.trigger) : null} : row.type === 'LimitOrderType' ? value.tif : value;
}
console.log(JSON.stringify({constants,cases,closedDeclarations:types,iterationStop:47}));
