import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(import.meta.url);
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'name_complete_typescript_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const relative = 'packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts';
const filename = path.join(root, relative);
const text = fs.readFileSync(filename, 'utf8');

function literalGetter(source, className) {
  const tree = ts.createSourceFile('proof.ts', source, ts.ScriptTarget.Latest, true);
  assert.equal(tree.parseDiagnostics.length, 0, 'name_complete_source_parse');
  const classes = tree.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === className);
  assert.equal(classes.length, 1, 'name_complete_source_class');
  const members = classes[0].members.filter(n => n.name?.getText(tree) === 'name');
  assert.equal(members.length, 1, 'name_complete_source_member');
  const getter = members[0];
  assert(ts.isGetAccessor(getter), 'name_complete_source_body_schema');
  assert.equal(getter.parameters.length, 0, 'name_complete_source_body_schema');
  assert.equal(getter.modifiers?.length ?? 0, 0, 'name_complete_source_body_schema');
  assert.equal(getter.body?.statements.length, 1, 'name_complete_source_body_schema');
  const statement = getter.body.statements[0];
  assert(ts.isReturnStatement(statement) && ts.isStringLiteral(statement.expression),
    'name_complete_source_body_schema');
  return statement.expression.text;
}

// This is a syntax proof obligation, not a replacement implementation.
// Any new read/call/branch/statement invalidates the one-equivalence-class argument.
const sourceLiteral = literalGetter(text, 'PolymarketExchange');

// Load the FULL maintained module and its real imports. No extracted class,
// rewritten function body, dependency stub, or fallback loader is admitted.
const { PolymarketExchange } = await import(pathToFileURL(filename).href);
const descriptor = Object.getOwnPropertyDescriptor(PolymarketExchange.prototype, 'name');
assert(descriptor && typeof descriptor.get === 'function' && descriptor.set === undefined,
  'name_complete_real_getter');
const getter = descriptor.get;
const runtimeLiteral = literalGetter(`class RuntimeGetter { ${Function.prototype.toString.call(getter)} }`,
  'RuntimeGetter');

let traps = 0;
const poison = new Proxy({}, {
  get() { traps++; throw Error('unexpected receiver/argument read'); },
  set() { traps++; throw Error('unexpected receiver/argument write'); },
  getPrototypeOf() { traps++; throw Error('unexpected receiver/argument prototype'); },
  ownKeys() { traps++; throw Error('unexpected receiver/argument enumeration'); },
});
const revoked = Proxy.revocable({}, {});
revoked.revoke();
const receivers = [
  ['undefined', undefined], ['null', null], ['false', false], ['true', true],
  ['positive_zero', 0], ['negative_zero', -0], ['nan', NaN],
  ['positive_infinity', Infinity], ['negative_infinity', -Infinity], ['number', 7],
  ['bigint', 7n], ['symbol', Symbol('receiver')], ['empty_string', ''],
  ['ascii_string', 'x'], ['lone_surrogate_string', '\ud800'], ['object', {}],
  ['null_proto_object', Object.create(null)], ['array', []], ['function', () => {}],
  ['poison_proxy', poison], ['revoked_proxy', revoked.proxy],
];
assert.equal(receivers.length, 21, 'name_complete_receiver_floor');
const cases = [];
const events = [];
queueMicrotask(() => events.push('outside_microtask'));
for (const [id, receiver] of receivers) {
  for (const [argsId, args] of [['no_args', []], ['extra_args', [poison, Symbol('argument'), undefined]]]) {
    const result = Reflect.apply(getter, receiver, args);
    // Runtime assertions precede JSON capture, and do not re-encode a native result.
    assert.equal(typeof result, 'string', 'name_complete_source_type');
    assert.equal(result, 'polymarket', 'name_complete_source_value');
    assert.equal(traps, 0, 'name_complete_no_receiver_or_argument_effect');
    assert.deepEqual(events, [], 'name_complete_synchronous_completion');
    cases.push({ id: `${id}/${argsId}`, result, type: typeof result });
  }
}
assert.equal(sourceLiteral, 'polymarket', 'name_complete_source_literal');
assert.equal(runtimeLiteral, sourceLiteral, 'name_complete_loaded_body_matches_source');
assert.equal(cases.length, 42, 'name_complete_runtime_floor');
events.push('all_calls_returned');
await Promise.resolve();
assert.deepEqual(events, ['all_calls_returned', 'outside_microtask'], 'name_complete_no_yield');

process.stdout.write(JSON.stringify({
  source: relative,
  sourceSha256: crypto.createHash('sha256').update(text).digest('hex'),
  capture: { node: process.versions.node, v8: process.versions.v8, architecture: process.arch,
    platform: process.platform, typescript: ts.version, loader: 'tsx/full-module' },
  proofClasses: 1, receiverWitnesses: 21, calls: 42, traps, cases, events,
  knownDifferenceFilters: 0,
}));
