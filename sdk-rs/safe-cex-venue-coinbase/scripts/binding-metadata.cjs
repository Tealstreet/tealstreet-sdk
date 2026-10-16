// Execute the actual three metadata initializers; only unused imported enum/IP
// values are supplied. No copy of the consumed settings or source booleans.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const filename = path.join(root, 'packages/safe-cex/src/exchangeMetadata.ts');
const text = fs.readFileSync(filename, 'utf8');
const source = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true);
const names = ['defaultMetaData', 'CoinbaseMetadata', 'CoinbaseSpotMetadata'];
const declarations = [];
for (const statement of source.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (names.includes(declaration.name.getText(source))) {
      if (!declaration.initializer) throw Error('missing metadata initializer');
      declarations.push(`const ${declaration.name.getText(source)} = ${declaration.initializer.getText(source)};`);
    }
  }
}
if (declarations.length !== names.length) throw Error('Coinbase metadata declaration census changed');
const code = ts.transpileModule(declarations.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const result = vm.runInNewContext(code + `\nJSON.stringify(Object.fromEntries([CoinbaseMetadata,CoinbaseSpotMetadata].map(m => [m.exchangeName, {tickInterval:m.tickInterval,supportsPublicOnly:m.supportsPublicOnly,positions:m.features.positions}])));`, {
  PositionMode: { OneWay: 'oneway', Hedged: 'hedged' }, MarginType: { Cross: 'cross', Isolated: 'isolated' }, privateProxyIpsSingapore: [],
});
process.stdout.write(result);
