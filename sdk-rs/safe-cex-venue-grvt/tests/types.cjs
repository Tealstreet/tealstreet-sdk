'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.types.ts');
let source = fs.readFileSync(file, 'utf8');
const builderFile = path.join(path.dirname(file), 'grvt.builder.ts');
const builderTree = ts.createSourceFile(builderFile, fs.readFileSync(builderFile, 'utf8'), ts.ScriptTarget.Latest, true);
const reason = builderTree.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'GrvtBuilderUnauthorizedReason');
assert(reason, 'actual builder reason declaration');
source += '\n' + reason.getText(builderTree);

const sharedFile = path.resolve(path.dirname(file), '../../types.ts');
const sharedTree = ts.createSourceFile(sharedFile, fs.readFileSync(sharedFile, 'utf8'), ts.ScriptTarget.Latest, true);
const sharedEnum = sharedTree.statements.find(s => ts.isEnumDeclaration(s) && s.name.text === 'OrderStopTriggerType');
assert(sharedEnum, 'actual shared trigger enum');
source += '\n' + sharedEnum.getText(sharedTree);
const triggerFile = path.join(path.dirname(file), 'grvt.order-semantics.ts');
const triggerTree = ts.createSourceFile(triggerFile, fs.readFileSync(triggerFile, 'utf8'), ts.ScriptTarget.Latest, true);
const triggerAlias = triggerTree.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'GrvtTriggerMetadata');
assert(triggerAlias, 'actual trigger metadata alias');
source += '\n' + triggerAlias.getText(triggerTree);

if (['heritage-fill', 'heritage-subscription'].includes(process.env.GRVT_TYPES_MUTATION)) {
  const target = process.env.GRVT_TYPES_MUTATION === 'heritage-fill' ? 'GrvtFill' : 'GrvtWebSocketSubscription';
  const needle = `interface ${target} {`;
  assert(source.includes(needle), 'heritage mutation matched');
  source = 'interface GrvtHiddenNumeric { hiddenNumeric: number; }\n' + source.replace(needle, `interface ${target} extends GrvtHiddenNumeric {`);
} else if (process.env.GRVT_TYPES_MUTATION === 'builder-reason') {
  assert(source.includes("'unauthorized' | 'cap-too-low'"), 'builder reason mutation matched');
  source = source.replace("'unauthorized' | 'cap-too-low'", "'blocked' | 'cap-too-low'");
} else if (process.env.GRVT_TYPES_MUTATION === 'side') {
  assert(source.includes("GrvtOrderSide = 'BUY' | 'SELL'"), 'side mutation matched');
  source = source.replace("GrvtOrderSide = 'BUY' | 'SELL'", "GrvtOrderSide = 'BID' | 'SELL'");
} else if (process.env.GRVT_TYPES_MUTATION === 'liquidity') {
  assert(source.includes("liquidity: 'MAKER' | 'TAKER'"), 'liquidity mutation matched');
  source = source.replace("liquidity: 'MAKER' | 'TAKER'", "liquidity: 'POSTER' | 'TAKER'");
} else if (process.env.GRVT_TYPES_MUTATION === 'rpc') {
  assert(source.includes("jsonrpc: '2.0'"), 'rpc mutation matched');
  source = source.replace("jsonrpc: '2.0'", "jsonrpc: '3.0'");
} else assert(!process.env.GRVT_TYPES_MUTATION, 'unknown types mutation');
const options = { strict: true, noLib: true, target: ts.ScriptTarget.ES2022 };
const host = ts.createCompilerHost(options);
const originalRead = host.readFile;
host.readFile = name => name === file ? source : originalRead(name);
const program = ts.createProgram([file], options, host);
assert.equal(program.getSyntacticDiagnostics().length, 0, 'protocol syntax');
const tree = program.getSourceFile(file);
for (const declaration of tree.statements.filter(ts.isInterfaceDeclaration)) {
  assert(!declaration.heritageClauses?.length, `GRVT union census refuses inherited fields: ${declaration.name.text}`);
}
const checker = program.getTypeChecker();
for (const name of ['GrvtOrderSide', 'GrvtOrderType', 'GrvtTimeInForce', 'GrvtCandlestickInterval', 'GrvtOrderStatus', 'GrvtAssetKind', 'GrvtLiquidity', 'GrvtJsonRpcVersion', 'GrvtSubscriptionMethod', 'GrvtBuilderUnauthorizedReason', 'GrvtTriggerKind']) {
  const declaration = name === 'GrvtTriggerKind'
    ? tree.statements.find(s => s.name?.getText(tree) === 'GrvtTriggerMetadata').type.members.find(m => m.name?.getText(tree) === 'type')
    : ['GrvtJsonRpcVersion','GrvtSubscriptionMethod'].includes(name)
    ? tree.statements.find(s => s.name?.getText(tree) === 'GrvtWebSocketSubscription').members.find(m => m.name?.getText(tree) === (name === 'GrvtJsonRpcVersion' ? 'jsonrpc' : 'method'))
    : name === 'GrvtLiquidity'
    ? tree.statements.find(s => s.name?.getText(tree) === 'GrvtFill').members.find(m => m.name?.getText(tree) === 'liquidity')
    : tree.statements.find(s => s.name?.getText(tree) === name);
  assert(declaration, `missing declaration ${name}`);
  const type = checker.getTypeAtLocation(declaration);
  assert(type.isUnion() || type.flags & ts.TypeFlags.StringLiteral, `closed union required: ${name}`);
  for (const member of type.isUnion() ? type.types : [type]) {
    assert(member.flags & ts.TypeFlags.StringLiteral, `literal member required: ${name}`);
    process.stdout.write(`${name}\t${member.value}\n`);
  }
}
