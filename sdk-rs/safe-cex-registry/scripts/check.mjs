import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const args = process.argv.slice(2);
const fixtures = new Map();
while (args.length) {
  const flag = args.shift();
  assert.ok(['--actual', '--native', '--gateway'].includes(flag) && args.length && !fixtures.has(flag), 'invalid oracle fixture arguments');
  fixtures.set(flag, args.shift());
}
const source = (name) => {
  const fixture = name === 'registry.native.ts' ? fixtures.get('--native') : name === 'external-public-data.ts' ? fixtures.get('--gateway') : undefined;
  return ts.createSourceFile(name, readFileSync(fixture ?? new URL(`../../../packages/safe-cex/src/exchanges/${name}`, import.meta.url), 'utf8'), ts.ScriptTarget.Latest, true);
};
const collect = (file) => {
  let properties;
  const walk = (node) => {
    if (ts.isVariableDeclaration(node) && node.name.getText(file) === 'exchangeRegistry') properties = node.initializer.properties;
    ts.forEachChild(node, walk);
  };
  walk(file);
  assert.ok(properties, 'registry declaration absent');
  return properties.map((property) => {
    let module, constructor;
    const scan = (node) => {
      if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword || node.expression.getText(file) === 'require')) module = node.arguments[0].text;
      if (ts.isPropertyAccessExpression(node) && node.name.text.endsWith('Exchange')) constructor = node.name.text;
      ts.forEachChild(node, scan);
    };
    scan(property.initializer);
    assert.ok(module && constructor, property.name.getText(file));
    const name = property.name.text;
    return { name, module, constructor };
  });
};
const web = collect(source('registry.ts'));
const native = collect(source('registry.native.ts'));
assert.deepEqual(native, web, 'web/native descriptors differ');
// The CI negative controls inject complete example output through this same checker.
let actual;
if (fixtures.has('--actual')) {
  actual = JSON.parse(readFileSync(fixtures.get('--actual'), 'utf8'));
} else {
  const result = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-registry', '--example', 'registry-data'], { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.error?.message ?? result.stderr);
  actual = JSON.parse(result.stdout);
}
assert.deepEqual(actual.registry, web, 'Rust registry differs from live TypeScript data');
assert.deepEqual(actual.names, web.map((entry) => entry.name), 'registered names/order differ');
const gateway = source('external-public-data.ts');
const channelDeclaration = gateway.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === 'GatewayChannel');
assert.ok(channelDeclaration && ts.isUnionTypeNode(channelDeclaration.type), 'GatewayChannel must remain a literal union');
const tsChannels = [...new Set(channelDeclaration.type.types.map(node => {
  assert.ok(ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal), 'GatewayChannel member must be a string literal');
  return node.literal.text;
}))];
assert.ok(!tsChannels.includes('auth'), 'declared auth divergence requires auth outside the TS union');
assert.deepEqual(actual.channels.filter(value => value !== 'auth').sort(), [...tsChannels].sort(), 'GatewayChannel members differ beyond declared Auth extension');
assert.equal(actual.channels.filter(value => value === 'auth').length, 1);
let authCast = false;
const findAuthCast = (node) => {
  if (ts.isPropertyAssignment(node) && node.name.getText(gateway) === 'channel' && ts.isAsExpression(node.initializer) && node.initializer.expression.text === 'auth' && node.initializer.type.getText(gateway) === 'GatewayChannel') authCast = true;
  ts.forEachChild(node, findAuthCast);
};
findAuthCast(gateway);
assert.ok(authCast, 'declared GatewayChannel auth divergence requires its source cast');
assert.deepEqual(actual.authReply, { type: 'subscribed', exchangeId: 'bybit', channel: 'auth' });
let messageCast;
const findMessageCast = node => {
  if (ts.isVariableDeclaration(node) && node.name.getText(gateway) === 'message' && node.initializer && ts.isAsExpression(node.initializer) && node.initializer.type.getText(gateway) === 'GatewayMessage') messageCast = node.initializer;
  ts.forEachChild(node, findMessageCast);
};
findMessageCast(gateway);
assert.ok(messageCast, 'whole-frame divergence requires the source GatewayMessage cast');
assert.equal(messageCast.expression.getText(gateway), 'jsonParse(event.data)', 'revisit divergence if source validates frames');
const decodeContext = vm.createContext({ jsonParse: JSON.parse });
vm.runInContext(ts.transpileModule(`globalThis.decode = (event) => (${messageCast.getText(gateway)});`, {}).outputText, decodeContext);
const nullFrame = { type: 'data', exchangeId: 'bybit', symbol: null };
assert.deepEqual(JSON.parse(JSON.stringify(decodeContext.decode({ data: JSON.stringify(nullFrame) }))), nullFrame, 'TS cast preserves the null field');
assert.equal(actual.rejectsNullSymbol, true, 'Rust rejects the same null field that the TS cast preserves');
const declarations = new Map();
const walk = (node) => { if (ts.isVariableDeclaration(node)) declarations.set(node.name.getText(gateway), node.initializer); ts.forEachChild(node, walk); };
walk(gateway);
for (const [key, name] of [['ping', 'EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS'], ['reconnect', 'EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS']]) {
  assert.equal(actual[key], Number(declarations.get(name).getText(gateway).replaceAll('_', '')), name);
}
const context = vm.createContext({});
const program = ['getCandleSubscriptionKey', 'parseCandleSubscriptionKey'].map((name) => `globalThis.${name} = ${declarations.get(name).getText(gateway)};`).join('\n');
vm.runInContext(ts.transpileModule(program, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
const expectedKeys = [['BTC:USDT', '1m'], ['A:B:C', '1h'], ['', '1M'], ['雪:😀', '5m']].map(([symbol, interval]) => {
  const key = context.getCandleSubscriptionKey(symbol, interval);
  return { key, parts: Array.from(context.parseCandleSubscriptionKey(key)) };
});
assert.deepEqual(actual.keys, expectedKeys, 'subscription key mapping differs');
const malformedTs = Array.from(context.parseCandleSubscriptionKey('😀'));
assert.equal(malformedTs[0].charCodeAt(0), 0xd83d, 'source UTF-16 high surrogate');
assert.deepEqual(actual.malformedAstral, malformedTs.map(value => value.toWellFormed()), 'source UTF-16 slicing with typed String replacement');
assert.notDeepEqual(actual.malformedAstral, malformedTs, 'typed String replacement of the isolated unit stays explicit');
console.log(`Registry equivalence: ${web.length} ordered names in both registries; thresholds and colon-bearing subscription keys match live TS.`);
