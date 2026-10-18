import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import test from 'node:test';
import { collectCallableDeclarations } from './store-notification-declarations.mjs';

const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const scriptDirectory = fileURLToPath(new URL('./', import.meta.url));
const root = fileURLToPath(new URL('../../../', import.meta.url));
const gateFile = path.join(scriptDirectory, 'check-store-notification.mjs');
const collectorFile = path.join(scriptDirectory, 'store-notification-declarations.mjs');
const reviewFile = path.join(scriptDirectory, 'store-notification-reviewed-roots.json');
const fixture = `
type Provider = () => Promise<string>;
type Transport = (request: unknown) => Promise<unknown>;
type Alias = Provider;
type CycleA = CycleB;
type CycleB = CycleA;
class Example {
  private readonly replaySubscriptions: () => Promise<void>;
  private readonly createCdpJwt: Provider;
  private readonly clock?: () => number;
  private readonly transport: Transport;
  private readonly alias: Alias;
  private readonly maybe: (() => void) | undefined;
  private readonly ordinary: number;
  private readonly notCallable: Record<string, Provider>;
  private readonly promise: Promise<Provider>;
  private readonly cycle: CycleA;
  private arrow = () => {};
  method() {}
  constructor(private readonly injected: Alias) {}
}`;
function declarations(collect = collectCallableDeclarations) {
  const source = ts.createSourceFile('fixture.ts', fixture, ts.ScriptTarget.Latest, true);
  assert.equal(source.parseDiagnostics.length, 0, 'callable property fixture parses');
  const found = [];
  collect(ts, 'fixture.ts', source, (_file, key, _node, _source, kind) => found.push({ key, kind }));
  return found;
}
function assertInjected(collect) {
  const keys = declarations(collect).filter((item) => item.kind === 'injected-function-property').map((item) => item.key);
  assert.deepEqual(keys, [
    'Example.replaySubscriptions', 'Example.createCdpJwt', 'Example.clock',
    'Example.transport', 'Example.alias', 'Example.maybe', 'Example.injected',
  ], 'injected callable declarations retain direct, optional, aliased and constructor-property forms');
}
function child(script, env = process.env) {
  const result = spawnSync(process.execPath, [script], { encoding: 'utf8', env, cwd: root });
  if (result.error) throw result.error;
  return result;
}
function control({ gate = fs.readFileSync(gateFile, 'utf8'), helper, review }, inspect) {
  const suffix = randomUUID();
  const files = [];
  const candidate = path.join(scriptDirectory, `.notification-control-${suffix}.mjs`);
  try {
    if (helper !== undefined) {
      const name = `.notification-collector-${suffix}.mjs`;
      files.push(path.join(scriptDirectory, name)); fs.writeFileSync(files.at(-1), helper);
      gate = gate.replace('./store-notification-declarations.mjs', `./${name}`);
    }
    if (review !== undefined) {
      const name = `.notification-review-${suffix}.json`;
      files.push(path.join(scriptDirectory, name)); fs.writeFileSync(files.at(-1), JSON.stringify(review));
      gate = gate.replace('./store-notification-reviewed-roots.json', `./${name}`);
    }
    files.push(candidate); fs.writeFileSync(candidate, gate);
    inspect(child(candidate));
  } finally { for (const file of files) fs.rmSync(file, { force: true }); }
}
function failsNamed(result, name) {
  assert.equal(result.status, 1, `negative control exits 1: ${name}`);
  assert(result.stderr.includes(name), `negative control reaches named assertion: ${name}`);
}
function healthControl(before, after, expected) {
  const original = fs.readFileSync(gateFile, 'utf8');
  const declaration = original.split('\n').find(line => line.startsWith('const rustHealth = sourceText('));
  assert(declaration, 'production health source reader');
  const gate = original.replace(declaration,
    declaration.slice(0, -1) + `.replace(${JSON.stringify(before)}, ${JSON.stringify(after)});`);
  control({gate}, result => failsNamed(result, expected));
}
test('health return carrier cannot hide behind fallible callback signatures', () => {
  healthControl(') -> Result<(), PortfolioFailure> {\n        let detail',
    ') -> Result<(), String> {\n        let detail',
    'mark_private_user_stream_auth_failed must expose the fallible Store contract');
});
test('health callbacks must retain the fallible native carrier', () => {
  healthControl('log: impl FnMut(&str, &str) -> Result<(), PortfolioFailure>',
    'log: impl FnMut(&str, &str)',
    'mark_private_user_stream_auth_failed must retain fallible log capability');
});
test('health Store notification origin cannot be flattened', () => {
  healthControl('.map_err(|error| Rc::new(PortfolioThrown::from(error)))?;',
    '.map_err(|_| Rc::new(PortfolioThrown::from_value(None)))?;',
    'health native origin bridge retains StoreMutationError');
});
test('health retained conversion must keep the original partial update', () => {
  healthControl('.update_socket_health(&Value::Object(update).into())',
    '.update_socket_health(&Value::Null.into())',
    'health native origin bridge retains StoreMutationError');
});
test('health log failures cannot be discarded', () => {
  healthControl('log(reason, "warning")?;', 'let _ = log(reason, "warning");',
    'health log failure must propagate');
});
test('health warning failures cannot be discarded', () => {
  healthControl('emit_warning(reason)?;', 'let _ = emit_warning(reason);',
    'health warning failure must propagate');
});
function reviewedWalk(result) {
  assert.equal(result.status, 0, result.stderr);
  const value = JSON.parse(result.stdout);
  assert.equal(value.auditedCallableRows, 262, 'independent reviewed root floor');
  assert.equal(value.matchedSourceDeclarations, 262, 'independent source declaration floor');
  assert.equal(value.portedCallableRows, 246, 'current ported cohort excludes fourteen findings and two reviewed primitive choices');
  assert.deepEqual(value.reviewedCryptoChoiceRoots.sort(), ['getSigningKey', 'sign'], 'both primitive choices retain notification review');
  assert.deepEqual(value.injectedCallableProperties.sort(), [
    'CoinbaseStarbaseAuthSession#clock', 'CoinbaseStarbaseAuthSession#createCdpJwt',
    'CoinbaseStarbaseJsonRpcClient#transport', 'CoinbaseStarbaseWebsocketSessionController#replaySubscriptions',
  ].sort(), 'four real injected callable property roots');
  assert.equal(value.reviewedNotificationFindingRoots.length, 14, 'independent finding root floor');
  assert.deepEqual([...value.reviewedNotificationFindingRoots].sort(), [
  "CoinbaseExchange._startPrivate",
  "CoinbaseExchange._startPublicDataOnly",
  "CoinbaseExchange.cancelOrders",
  "CoinbaseExchange.placeOrder",
  "CoinbaseExchange.reconcileOpenOrdersFromRest",
  "CoinbaseExchange.scheduleOpenOrdersRestReconciliation",
  "CoinbaseExchange.schedulePrivateUserStreamWriteCheck",
  "CoinbaseExchange.start",
  "CoinbaseExchange.tick",
  "CoinbaseExchange.updateOrder",
  "CoinbasePrivateWebsocket.handlePrivateStreamError",
  "CoinbasePrivateWebsocket.handleUserEvent",
  "CoinbasePrivateWebsocket.handleUserEventOrders",
  "CoinbasePrivateWebsocket.onMessage"
], 'independent notification finding identities');
  assert.equal(value.reviewedNotificationFindingChains.length, 29, 'independent reviewed reachability floor');
  assert.equal(value.directStoreNotificationCalls.length, 13, 'independent direct Store call floor');
  assert.equal(value.distinctWriterRows.length, 10, 'independent direct writer row floor');
  assert.deepEqual(value.reachableStoreNotificationChains, [], 'no unhandled ported notification chain');
}

test('typed callable declaration inventory covers all injected forms without inventing ordinary properties', () => {
  assertInjected(collectCallableDeclarations);
  assert.deepEqual(declarations().filter((item) => item.kind === 'body').map((item) => item.key), ['Example.arrow', 'Example.method', 'Example.']);
});
test('full notification audit walks all 262 reviewed roots', () => reviewedWalk(child(gateFile)));
test('omitting injected properties fails the real declaration assertion; restoring passes', () => {
  const helper = fs.readFileSync(collectorFile, 'utf8').replace('else if (callableType(member.type))', 'else if (false && callableType(member.type))');
  control({ helper }, (result) => failsNamed(result, 'every reviewed callable resolves to a source declaration'));
  reviewedWalk(child(gateFile));
});
test('omitting callable aliases fails the real declaration assertion; restoring passes', () => {
  const helper = fs.readFileSync(collectorFile, 'utf8').replace('return callableType(aliases.get(name), new Set([...visiting, name]));', 'return false;');
  control({ helper }, (result) => failsNamed(result, 'every reviewed callable resolves to a source declaration'));
  reviewedWalk(child(gateFile));
});
test('stale 159-root census fails rather than shrinking the walk', () => {
  const gate = fs.readFileSync(gateFile, 'utf8').replace('expectedAuditedCallableRows = 262', 'expectedAuditedCallableRows = 159');
  control({ gate }, (result) => failsNamed(result, 'reviewed callable corpus total'));
});
test('a same-length reviewed-root replacement cannot erase a ported callable', () => {
  const review = JSON.parse(fs.readFileSync(reviewFile, 'utf8'));
  review.roots.find((row) => row.review === 'ported').name = 'missingCallable';
  assert.equal(review.roots.length, 262, 'same-length mutation keeps independent corpus length');
  control({ review }, (result) => failsNamed(result, 'every ported callable has notification review coverage'));
});
test('a crypto choice cannot erase or reclassify another audited root', () => {
  const review = JSON.parse(fs.readFileSync(reviewFile, 'utf8'));
  review.roots.find((row) => row.name === 'CoinbaseExchange.tick').review = 'crypto-choice';
  control({ review }, (result) => failsNamed(result, 'only reviewed crypto roots use crypto-choice: CoinbaseExchange.tick'));
  reviewedWalk(child(gateFile));
});
test('an unaudited new direct Store writer is refused', () => {
  const original = "const sourceText = (url) => fs.readFileSync(url, 'utf8');";
  const replacement = `const sourceText = (url) => {
    const text = fs.readFileSync(url, 'utf8');
    return String(url).endsWith('coinbase.api.ts')
      ? text.replace('export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {', 'export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => { exchange.store.update({});')
      : text;
  };`;
  const gate = fs.readFileSync(gateFile, 'utf8').replace(original, replacement);
  assert.notEqual(gate, fs.readFileSync(gateFile, 'utf8'), 'writer control changes the in-memory source reader');
  control({ gate }, (result) => failsNamed(result, 'Store writer cannot claim notification parity: createAPI'));
});
test('a changed reviewed source-site fingerprint is refused', () => {
  const review = JSON.parse(fs.readFileSync(reviewFile, 'utf8'));
  review.directStoreCalls[0].sourceSha256 = '0'.repeat(64);
  control({ review }, (result) => failsNamed(result, 'every direct Store call matches its reviewed source site'));
});
test('an actual new direct call inside an acknowledged finding is refused', () => {
  const original = "const sourceText = (url) => fs.readFileSync(url, 'utf8');";
  const replacement = `const sourceText = (url) => {
    const text = fs.readFileSync(url, 'utf8');
    return String(url).endsWith('coinbase.exchange.ts')
      ? text.replace('tick = async () => {', 'tick = async () => { this.store.update({ orders: [] });')
      : text;
  };`;
  const gate = fs.readFileSync(gateFile, 'utf8').replace(original, replacement);
  assert.notEqual(gate, fs.readFileSync(gateFile, 'utf8'), 'finding writer control changes the source reader');
  control({ gate }, (result) => failsNamed(result, 'reviewed direct Store call floor'));
  reviewedWalk(child(gateFile));
});
test('promoting an unsupported finding to ported is refused before the walk can go green', () => {
  const gate = fs.readFileSync(gateFile, 'utf8').replace("status: parseField(block, 'status'),", "status: parseField(block, 'ts') === 'CoinbaseExchange.tick' ? 'ported' : parseField(block, 'status'),");
  control({ gate }, (result) => failsNamed(result, 'reviewed callable status: CoinbaseExchange.tick'));
});
test('dropping the repaired snapshot native origin is refused', () => {
  const original = "const sourceText = (url) => fs.readFileSync(url, 'utf8');";
  const replacement = `const sourceText = (url) => {
    const text = fs.readFileSync(url, 'utf8');
    return String(url).endsWith('portfolios_balances.rs')
      ? text.replace('store_mutation: Some(error)', 'store_mutation: None')
      : text;
  };`;
  const gate = fs.readFileSync(gateFile, 'utf8').replace(original, replacement);
  assert.notEqual(gate, fs.readFileSync(gateFile, 'utf8'), 'native-origin control changes the source reader');
  control({ gate }, (result) => failsNamed(result, 'snapshot native origin bridge retains StoreMutationError'));
  reviewedWalk(child(gateFile));
});
test('a no-op walk cannot pass the child audit assertion', () => {
  control({ gate: 'console.log(JSON.stringify({}));' }, (result) => assert.throws(() => reviewedWalk(result), /independent reviewed root floor/));
});
test('missing isolated compiler fails loudly', () => {
  const result = child(gateFile, { ...process.env, SAFE_CEX_TYPESCRIPT_PATH: 'missing-coinbase-notification-typescript-control' });
  assert.equal(result.status, 1, 'missing compiler exits 1');
  assert.match(result.stderr, /MODULE_NOT_FOUND/, 'missing compiler is not treated as a skipped gate');
});
test('audit resolves isolated TypeScript from a mirror without ancestor node_modules', () => {
  assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolation gate requires SAFE_CEX_TYPESCRIPT_PATH');
  const mirror = fs.mkdtempSync(path.join(os.tmpdir(), 'coinbase-notification-isolation-'));
  try {
    for (let ancestor = mirror; ; ancestor = path.dirname(ancestor)) {
      assert(!fs.existsSync(path.join(ancestor, 'node_modules')), 'mirror has no ancestor node_modules');
      if (path.dirname(ancestor) === ancestor) break;
    }
    for (const directory of ['packages/safe-cex/src/exchanges/coinbase', 'sdk-rs/parity/manifest/exchanges/coinbase']) {
      fs.mkdirSync(path.dirname(path.join(mirror, directory)), { recursive: true });
      fs.cpSync(path.join(root, directory), path.join(mirror, directory), { recursive: true });
    }
    for (const relative of [
      'sdk-rs/safe-cex-venue-coinbase/src/private_health.rs', 'sdk-rs/safe-cex-venue-coinbase/src/private_order_snapshot.rs',
      'sdk-rs/safe-cex-venue-coinbase/src/portfolios_balances.rs',
      ...['check-store-notification.mjs', 'store-notification-declarations.mjs', 'store-notification-reviewed-roots.json'].map((name) => `sdk-rs/safe-cex-venue-coinbase/scripts/${name}`),
    ]) {
      fs.mkdirSync(path.dirname(path.join(mirror, relative)), { recursive: true });
      fs.copyFileSync(path.join(root, relative), path.join(mirror, relative));
    }
    reviewedWalk(child(path.join(mirror, 'sdk-rs/safe-cex-venue-coinbase/scripts/check-store-notification.mjs')));
  } finally { fs.rmSync(mirror, { recursive: true, force: true }); }
});
