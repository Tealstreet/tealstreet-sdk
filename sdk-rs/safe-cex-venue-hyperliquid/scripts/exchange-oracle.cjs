const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts');
const names = ['parseFiniteNumber', 'maskHyperliquidAddress', 'parseHyperliquidExchangePayload', 'calculatePriceChangePercentage', 'parseHyperliquidVolumeFields', 'parseHyperliquidOpenInterestNotional', 'isFinitePositiveNumber'];
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const selected = ast.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => names.includes(d.name.text)));
if (selected.length !== names.length) throw Error('missing exchange helper source');
const printer = ts.createPrinter();
const helperCode = selected.map(s => printer.printNode(ts.EmitHint.Unspecified, s, ast)).join('\n');
const keyFile = path.join(root, 'packages/safe-cex/src/utils/get-key.ts');
const keyExports = {};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(keyFile, 'utf8'), {compilerOptions: {module: ts.ModuleKind.CommonJS, esModuleInterop: true}}).outputText, {exports: keyExports, require});
const exportsOut = {};
vm.runInNewContext(ts.transpileModule(helperCode, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText + '\n' + names.map(n => `out.${n}=${n};`).join('\n'), {out: exportsOut, v: keyExports.v});
const rows = [];
function capture(operation, raw) {
  const row = {operation, raw};
  try {const value = exportsOut[operation](...JSON.parse(raw)); if (value === undefined) row.undefined = true; else row.value = value;}
  catch (e) {row.error = e.message; row.errorName = e.name;}
  rows.push(row);
}
for (const raw of ['null','false','true','0','-0','1e400','-1e400','"Infinity"','"-Infinity"','"1e3tail"','"1e+tail"','"0x20"','".5rest"','"-.25x"','"+1.2E-3junk"','"\\ufeff12tail"','"\\u008512"','[]','[12,"34"]','{}','""','"NaN"']) {
  capture('parseFiniteNumber', '[' + raw + ']');
  capture('isFinitePositiveNumber', '[' + raw + ']');
}
for (const raw of ['null','""','123','"0x1234567890123"','"0x12345678901234"','"wallet-address-long-enough"','"ééééééééééééééé"']) capture('maskHyperliquidAddress', '[' + raw + ']');
for (const raw of ['null','{}','[]','true','123','"{}"','"[]"','"null"','"true"','"1"','"broken"']) capture('parseHyperliquidExchangePayload', '[' + raw + ']');
for (const raw of ['[110,100]','[0,100]','[10,0]','[10,-1]','[1e400,1]','[1,1e400]','[1e308,1e-308]']) capture('calculatePriceChangePercentage', raw);
for (const raw of ['[{"dayNtlVlm":"100","dayBaseVlm":"4","openInterest":"3"},20]', '[{"day_ntl_vlm":"100","day_base_vlm":"0","open_interest":"2"},20]', '[{"dayNtlVlm":null,"day_ntl_vlm":"500","dayBaseVlm":"-1"},0]', '[{},-1]', '[null,100]', '[123,100]']) {
  capture('parseHyperliquidVolumeFields', raw); capture('parseHyperliquidOpenInterestNotional', raw);
}
const corpus = JSON.stringify(rows, null, 2) + '\n';
const fixture = path.join(__dirname, '../tests/fixtures/exchange-helpers.json');
if (process.argv.includes('--capture')) fs.writeFileSync(fixture, corpus, {flag: 'wx'});
else if (fs.readFileSync(fixture, 'utf8') !== corpus) throw Error('live exchange helper oracle drift');
