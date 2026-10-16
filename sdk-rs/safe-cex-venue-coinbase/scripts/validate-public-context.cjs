const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-public-frames.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
const declarations = new Map();
for (const statement of source.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (ts.isIdentifier(declaration.name)) declarations.set(declaration.name.text, declaration);
  }
}
function declaration(name) {
  const found = declarations.get(name);
  assert.ok(found?.initializer, `source ${name} declaration must exist`);
  return found.initializer.getText(source);
}
const executable = [
  `const PUBLIC_INSTRUMENTS = ${declaration('PUBLIC_INSTRUMENTS')};`,
  `const BOOK_INTERVALS = ${declaration('BOOK_INTERVALS')};`,
  `const CHART_RESOLUTIONS = ${declaration('CHART_RESOLUTIONS')};`,
  `const isSupportedPublicChannel = ${declaration('isSupportedPublicChannel')};`,
  `const validateContext = ${declaration('validateContext')};`,
  'class CoinbaseStarbaseProtocolError extends Error { constructor(message) { super(message); this.name = "CoinbaseStarbaseProtocolError"; } }',
  'exportsObject.validateContext = validateContext;',
].join('\n');
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(executable, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exportsObject });

const valid = {
  acknowledgedPublicChannels: ['ticker.BTC_USDC-PERPETUAL.100ms'],
  desiredPublicChannels: ['chart.trades.ETH_USDC-PERPETUAL.5'],
  pendingPublicTestRequestIds: [1, 2],
};
const cases = [
  ['empty-valid', { acknowledgedPublicChannels: [], desiredPublicChannels: [], pendingPublicTestRequestIds: [] }],
  ['valid-lists', valid],
  ['missing-acknowledged', { desiredPublicChannels: [], pendingPublicTestRequestIds: [] }],
  ['wrong-acknowledged-container', { acknowledgedPublicChannels: null, desiredPublicChannels: [], pendingPublicTestRequestIds: [] }],
  ['unsupported-acknowledged', { ...valid, acknowledgedPublicChannels: ['ticker.DOGE_USDC-PERPETUAL.100ms'] }],
  ['wrong-acknowledged-member', { ...valid, acknowledgedPublicChannels: [null] }],
  ['duplicate-acknowledged', { ...valid, acknowledgedPublicChannels: ['ticker.BTC_USDC-PERPETUAL.100ms', 'ticker.BTC_USDC-PERPETUAL.100ms'] }],
  ['wrong-desired-container', { ...valid, desiredPublicChannels: {} }],
  ['unsupported-desired', { ...valid, desiredPublicChannels: ['ticker.BTC_USDC-PERPETUAL.250ms'] }],
  ['duplicate-desired', { ...valid, desiredPublicChannels: ['book.ETH_USDC-PERPETUAL.agg2', 'book.ETH_USDC-PERPETUAL.agg2'] }],
  ['wrong-id-container', { ...valid, pendingPublicTestRequestIds: null }],
  ['zero-id', { ...valid, pendingPublicTestRequestIds: [0] }],
  ['negative-zero-id', { ...valid, pendingPublicTestRequestIds: [-0] }],
  ['negative-id', { ...valid, pendingPublicTestRequestIds: [-1] }],
  ['fractional-id', { ...valid, pendingPublicTestRequestIds: [1.5] }],
  ['unsafe-id', { ...valid, pendingPublicTestRequestIds: [9007199254740992] }],
  ['duplicate-id', { ...valid, pendingPublicTestRequestIds: [4, 4] }],
  ['acknowledged-error-precedence', { ...valid, acknowledgedPublicChannels: null, desiredPublicChannels: null, pendingPublicTestRequestIds: null }],
  ['desired-error-precedence', { ...valid, desiredPublicChannels: null, pendingPublicTestRequestIds: null }],
];
function capture(value) {
  try {
    exportsObject.validateContext(value);
    return {};
  } catch (error) {
    return { error: { name: error.name, message: error.message } };
  }
}
process.stdout.write(JSON.stringify({
  cases: cases.map(([label, input]) => ({ label, input, result: capture(input) })),
}));
