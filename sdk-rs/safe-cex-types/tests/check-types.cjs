// Read the current TS declarations directly; no frozen copy of the baseline.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const { dependencies, checkFields, normalized, shape } = require('./parity.cjs');
const { stringLiterals } = require('../generate.cjs');
const crate = path.resolve(__dirname, '..');
const baselinePath = path.resolve(crate, '../../packages/safe-cex/src/types.ts');
const models = ["OrderRiskProfile", "CoinBalance", "Balance", "Ticker", "PublicTrade", "OptionChainQuery", "OptionChainRow", "AccountGreeks", "Candle", "Execution", "OrderBookOrders", "OrderBook", "PlaceOrderOpts", "HistoricalTrade", "HistoricalTradeOptions", "OHLCVOptions", "OHLCVSOptions", "ExecutionsOptions", "OrderHistoryOptions", "HistoricalOrder", "StoreDataLoaded", "StoreOptions", "FetchedLeverage", "DisplayedMargin", "IncomeHistoryOptions", "Order", "Position", "Market", "ExchangeAccountConfig", "ExchangeOptions", "ValidateAccountResult", "UpdateOrderOpts", "ExchangeAccount", "ClosedPnlEntry", "IncomeRecord", "StoreUpdateIds", "PublicExchangeOptions"];
const parse = (file) => ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const baseline = parse(baselinePath);
const boundaries = ['RemoteSigner', 'PayloadSigner', 'TransportProvider', 'PublicDataSource'];
const { declarations, enums } = dependencies(baseline, ["OrderRiskProfile", "CoinBalance", "Balance", "Ticker", "PublicTrade", "OptionChainQuery", "OptionChainRow", "AccountGreeks", "Candle", "Execution", "OrderBookOrders", "OrderBook", "PlaceOrderOpts", "HistoricalTrade", "HistoricalTradeOptions", "OHLCVOptions", "OHLCVSOptions", "ExecutionsOptions", "OrderHistoryOptions", "HistoricalOrder", "StoreDataLoaded", "StoreOptions", "FetchedLeverage", "DisplayedMargin", "IncomeHistoryOptions", "Order", "Position", "Market", "ExchangeAccountConfig", "ExchangeOptions", "ValidateAccountResult", "UpdateOrderOpts", "ExchangeAccount", "ClosedPnlEntry", "IncomeRecord", "StoreUpdateIds", "PublicExchangeOptions", "ExchangeName", "DataProvenance", "IncomeCategory", "RequestOutcome", "RequestOutcomeCounts", "MessageHandlerStatsType", "SecondsTimeframe", "ExtendedTimeframe", "AnyTimeframe"], boundaries);
const index = new Map(declarations.map(d => [d.name.text, d]));
const rawFiles = fs.readdirSync(path.join(crate, 'bindings', 'raw')).filter(f => f.endsWith('.ts')).sort();
const rawDeclarations = rawFiles.map(file => {
  const source = parse(path.join(crate, 'bindings', 'raw', file));
  const declaration = source.statements.find(ts.isTypeAliasDeclaration);
  assert(declaration, `${file}: expected a raw declaration`);
  return declaration;
});
assert.deepEqual(rawDeclarations.map(d => d.name.text).sort(), declarations.map(d => d.name.text).sort(), 'raw exports must equal the oracle dependency graph');
const rawEnums = rawDeclarations.filter(d => enums.includes(d.name.text)).map(d => d.name.text).sort();
assert.deepEqual(rawEnums, [...enums].sort(), 'all raw enum exports must be checked');
assert.deepEqual(Object.keys(JSON.parse(fs.readFileSync(path.join(crate, 'bindings', 'enum-members.json'), 'utf8'))).sort(), rawEnums, 'Rust enum metadata must cover every raw enum');
const unionMetadata = JSON.parse(fs.readFileSync(path.join(crate, 'bindings/string-unions.json'), 'utf8'));
const aliases = declarations.filter(d => ts.isTypeAliasDeclaration(d) && !models.includes(d.name.text));
const literalAliases = aliases.filter(d => stringLiterals(d.type));
assert.deepEqual(Object.keys(unionMetadata).sort(), literalAliases.map(d => d.name.text).sort(), 'Rust literal-alias intent must equal oracle literal aliases');
for (const expected of aliases) {
  const name = expected.name.text;
  const actual = parse(path.join(crate, 'bindings', `${name}.ts`)).statements.find(d => d.name?.text === name);
  assert(ts.isTypeAliasDeclaration(actual), `${name}: literal/type alias must remain a type alias`);
  assert.deepEqual(normalized(actual.type), normalized(expected.type), `${name}: alias type`);
}
const printer = ts.createPrinter();
let fields = 0;
for (const name of models) {
  const expected = declarations.find(s => s.name.text === name);
  const generated = parse(path.join(crate, 'bindings', `${name}.ts`));
  fields += checkFields(shape(expected, index), generated.statements.find(s => s.name?.text === name).type, name);
}
for (const name of enums) {
  const expected = declarations.find(s => s.name.text === name).members.map(m => [m.name.text, m.initializer.text]);
  const generated = parse(path.join(crate, 'bindings', `${name}.ts`));
  const declaration = generated.statements.find(s => s.name?.text === name);
  assert(ts.isEnumDeclaration(declaration), `${name}: enum declaration required`);
  assert(declaration.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword), `${name}: exported enum required`);
  assert(!declaration.modifiers?.some(m => [ts.SyntaxKind.ConstKeyword, ts.SyntaxKind.DeclareKeyword].includes(m.kind)), `${name}: runtime enum required`);
  assert(declaration.members.every(m => m.initializer && ts.isStringLiteral(m.initializer)), `${name}: string values required`);
  assert.deepEqual(declaration.members.map(m => [m.name.text, m.initializer.text]), expected, `${name}: member names/values`);
  const raw = parse(path.join(crate, 'bindings', 'raw', `${name}.ts`));
  const union = raw.statements.find(s => s.name?.text === name).type;
  assert.deepEqual(stringLiterals(union), expected.map(([, value]) => value), `${name}: raw values`);
  const runtime = {};
  const compiled = ts.transpileModule(generated.getFullText(), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  });
  new Function('exports', compiled.outputText)(runtime);
  assert.deepEqual(runtime[name], Object.fromEntries(expected), `${name}: runtime members`);
}
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'safe-cex-types-'));
try {
  fs.writeFileSync(path.join(temp, 'legacy.ts'), `import type { ${boundaries.join(', ')} } from '${path.join(crate, 'bindings/runtime-boundaries')}';\n` + declarations.map(d => printer.printNode(ts.EmitHint.Unspecified, d, baseline)).join('\n'));
  const checks = declarations.filter(d => !ts.isEnumDeclaration(d)).map(d => d.name.text).map(name => `
import type { ${name} as Generated${name} } from '${path.join(crate, 'bindings', name)}';
declare const legacy${name}: Legacy.${name};
declare const generated${name}: Generated${name};
const toGenerated${name}: Generated${name} = legacy${name};
const toLegacy${name}: Legacy.${name} = generated${name};

`).join('\n');
  const enumChecks = enums.map(name => {
    const members = declarations.find(s => s.name.text === name).members;
    return `import { ${name} as Generated${name} } from '${path.join(crate, 'bindings', name)}';
declare const legacy${name}: Legacy.${name};
declare const generated${name}: Generated${name};
const enumToGenerated${name}: Generated${name} = legacy${name};
const enumToLegacy${name}: Legacy.${name} = generated${name};
${members.map(m => `
const runtime${name}${m.name.text}: ${JSON.stringify(m.initializer.text)} = Generated${name}.${m.name.text};
const memberToLegacy${name}${m.name.text}: Legacy.${name} = Generated${name}.${m.name.text};
const memberToGenerated${name}${m.name.text}: Generated${name} = Legacy.${name}.${m.name.text};
// @ts-expect-error Bare literals must not replace generated enum members.
const bareGenerated${name}${m.name.text}: Generated${name} = ${JSON.stringify(m.initializer.text)};
// @ts-expect-error Baseline also rejects bare literals.
const bareLegacy${name}${m.name.text}: Legacy.${name} = ${JSON.stringify(m.initializer.text)};
`).join('\n')}`;
  }).join('\n');
  fs.writeFileSync(path.join(temp, 'checks.ts'), `import * as Legacy from './legacy';\n${checks}\n${enumChecks}
// @ts-expect-error Nested properties are readonly.
generatedMarket.limits.amount.min = 1;
// @ts-expect-error Nested optional properties are readonly.
generatedOrder.extra!.chaserId = 'changed';
// @ts-expect-error Required numeric properties cannot be undefined.
const missing: GeneratedPosition = { ...generatedPosition, liquidationPrice: undefined };
// @ts-expect-error Optional numeric properties do not accept null.
const nullable: GeneratedOrder = { ...generatedOrder, stopLoss: null };
// @ts-expect-error Ticker derivative metadata is required even for spot data.
const unavailableFunding: GeneratedTicker = { ...generatedTicker, fundingTime: undefined };
const bothLeg: GeneratedExecution = { ...generatedExecution, positionSide: 'both' };
// @ts-expect-error Long is still an enum member, not a bare literal.
const bareLeg: GeneratedExecution = { ...generatedExecution, positionSide: 'long' };
const timeframeLiteral: GeneratedTimeframe = '1m';
const exchangeLiteral: GeneratedExchangeName = 'bybit';
generatedOrderBookOrders.price = 1;
generatedExchangeAccountConfig.name = 'changed';
generatedExchangeOptions.auth!.userAddress = 'changed';
generatedExchangeOptions.extra!.tickInterval = 1;
if (generatedExchangeOptions.externalPublicData?.type === 'websocket') {
  generatedExchangeOptions.externalPublicData.url = 'changed';
}
// @ts-expect-error Readonly resumes inside the otherwise mutable extra object.
generatedExchangeOptions.extra!.kucoinBroker!.partner = 'changed';
`);
  const compilerOptions = {
    strict: true, noEmit: true, skipLibCheck: true, types: [], target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.CommonJS, moduleResolution: ts.ModuleResolutionKind.Node10,
    ignoreDeprecations: '6.0',
  };
  // Assignment fixture covers finite data; distinct opaque capabilities preserve identity.
  const host = ts.createCompilerHost(compilerOptions);
  const getSourceFile = host.getSourceFile.bind(host);
  const boundaryPath = path.join(crate, 'bindings/runtime-boundaries.ts');
  host.getSourceFile = (file, ...args) => file === boundaryPath
    ? ts.createSourceFile(file, boundaries.map(name => `declare const ${name}Brand: unique symbol; export type ${name} = { readonly [${name}Brand]: never };`).join('\n'), compilerOptions.target, true)
    : getSourceFile(file, ...args);
  const program = ts.createProgram([path.join(temp, 'checks.ts')], compilerOptions, host);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: f => f, getCurrentDirectory: () => temp, getNewLine: () => '\n',
  }));
  const side = declarations.find(s => s.name.text === 'OrderSide');
  const [member] = side.members;
  fs.writeFileSync(path.join(temp, 'union-control.ts'),
    fs.readFileSync(path.join(crate, 'bindings', 'raw', 'OrderSide.ts'), 'utf8') +
    `\nexport const OrderSide = { ${side.members.map(m => `${m.name.text}: ${JSON.stringify(m.initializer.text)}`).join(', ')} } as const;\n`);
  fs.writeFileSync(path.join(temp, 'bare-control.ts'), `import { OrderSide } from './union-control';
const runtimeMember: ${JSON.stringify(member.initializer.text)} = OrderSide.${member.name.text};
// @ts-expect-error This guard must fail for a union plus const object.
const bare: OrderSide = ${JSON.stringify(member.initializer.text)};
`);
  const control = ts.getPreEmitDiagnostics(ts.createProgram([path.join(temp, 'bare-control.ts')], compilerOptions));
  assert.deepEqual(control.map(d => d.code), [2578], 'union+const negative control must expose bare-literal permissiveness');
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
console.log(`Parity passed: ${fields} fields, ${enums.length} real enums; member names/values/runtime, bidirectional assignments and bare-literal rejection verified; union+const negative control detected widening.`);
