// AST call-reachability census for the Coinbase Store notification seam.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { collectCallableDeclarations } from './store-notification-declarations.mjs';

const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');

const root = new URL('../../../', import.meta.url);
const manifestRoot = new URL('sdk-rs/parity/manifest/exchanges/coinbase/', root);
const sourceRoot = new URL('packages/safe-cex/src/exchanges/coinbase/', root);
const callableKinds = new Set(['function', 'method', 'field-fn', 'accessor', 'const-fn', 'variable-fn']);
// The complete reviewed cohort includes explicit findings; reclassification must
// not silently remove previously ported roots from the notification walk.
const expectedAuditedCallableRows = 262;
const review = JSON.parse(fs.readFileSync(new URL('./store-notification-reviewed-roots.json', import.meta.url), 'utf8'));
const reviewedRoots = review.roots;
// #148 deliberately classifies the production primitive's platform-error and
// nonce output choices. Keep these exact roots in the complete notification walk.
const reviewedCryptoChoices = new Set([
  'packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts::getSigningKey',
  'packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts::sign',
]);
const reviewedNotificationRows = new Set([
  'CoinbasePrivateWebsocket.applyOrderSnapshotBatch',
  'CoinbaseExchange.markPrivateUserStreamMessageReceived',
  'CoinbaseExchange.markPrivateUserStreamDegraded',
  'CoinbaseExchange.markPrivateUserStreamAuthFailed',
]);
const storeMutators = new Set([
  'update',
  'setSetting',
  'updateSocketHealth',
  'updateOrdersStatus',
  'addOrUpdateOrder',
  'addOrUpdateOrders',
  'recordRequestOutcome',
  'updateStarkConfig',
  'updateZkLinkConfig',
  'updateUserData',
]);
const declarations = [];
const directWrites = [];
const symbols = [];
const sourceText = (url) => fs.readFileSync(url, 'utf8');
const parseField = (block, key) => {
  const match = block.match(new RegExp(`^${key} = "([^"]*)"$`, 'm'));
  assert(match, `manifest row missing ${key}`);
  return match[1];
};
const canonicalSymbol = (name) => name.replace('#', '.');

function addDeclaration(file, key, node, source, kind = 'body') {
  const declaration = { file, key, kind, calls: new Set(), localStoreAliases: new Set(), localMutatorAliases: new Map() };
  declarations.push(declaration);
  function isStoreReceiver(expression) {
    if (ts.isPropertyAccessExpression(expression)) {
      return expression.name.text === 'store'
        || (ts.isIdentifier(expression.expression) && declaration.localStoreAliases.has(expression.expression.text));
    }
    if (ts.isElementAccessExpression(expression) && expression.argumentExpression
      && ts.isStringLiteralLike(expression.argumentExpression)) {
      return expression.argumentExpression.text === 'store'
        || (ts.isIdentifier(expression.expression) && declaration.localStoreAliases.has(expression.expression.text));
    }
    if (ts.isIdentifier(expression)) return declaration.localStoreAliases.has(expression.text);
    return false;
  }
  function storeMutatorName(expression) {
    if (ts.isPropertyAccessExpression(expression) && isStoreReceiver(expression.expression)) {
      return expression.name.text;
    }
    if (ts.isElementAccessExpression(expression) && isStoreReceiver(expression.expression)
      && expression.argumentExpression && ts.isStringLiteralLike(expression.argumentExpression)) {
      return expression.argumentExpression.text;
    }
    if (ts.isIdentifier(expression) && declaration.localMutatorAliases.has(expression.text)) {
      return declaration.localMutatorAliases.get(expression.text);
    }
    return undefined;
  }
  function collectAliases(current) {
    if (ts.isVariableDeclaration(current) && ts.isIdentifier(current.name) && current.initializer) {
      if (isStoreReceiver(current.initializer)) declaration.localStoreAliases.add(current.name.text);
      const mutator = storeMutatorName(current.initializer);
      if (mutator) declaration.localMutatorAliases.set(current.name.text, mutator);
    }
    ts.forEachChild(current, collectAliases);
  }
  collectAliases(node);
  function visit(current) {
    if (ts.isCallExpression(current)) {
      const expression = current.expression.getText(source);
      const terminal = expression.split('.').at(-1);
      declaration.calls.add(terminal);
      const mutator = storeMutatorName(current.expression);
      if (mutator && storeMutators.has(mutator)) {
        directWrites.push({ declaration, call: expression, syntax: current.getText(source) });
      }
    }
    ts.forEachChild(current, visit);
  }
  visit(node);
}

// Keep the less obvious write shapes covered: computed Store member and a
// locally aliased mutator must be classified as direct notifications too.
for (const snippet of [
  'function fixture() { this.store["update"]({}); }',
  'function fixture() { const notify = this.store.update; notify({}); }',
  'function fixture() { const store = this["store"]; store.update({}); }',
]) {
  const source = ts.createSourceFile('fixture.ts', snippet, ts.ScriptTarget.Latest, true);
  const before = directWrites.length;
  addDeclaration('fixture.ts', 'fixture', source.statements[0], source);
  assert.equal(directWrites.length, before + 1, `Store writer form was not recognized: ${snippet}`);
  directWrites.pop();
  declarations.pop();
}

for (const entry of fs.readdirSync(manifestRoot).filter((name) => name.endsWith('.toml')).sort()) {
  const manifest = sourceText(new URL(entry, manifestRoot));
  const file = parseField(manifest.slice(0, manifest.indexOf('[[symbol]]')), 'ts');
  const sourceUrl = new URL(file.replace('packages/safe-cex/src/exchanges/coinbase/', ''), sourceRoot);
  const source = ts.createSourceFile(file, sourceText(sourceUrl), ts.ScriptTarget.Latest, true);
  const rows = manifest.split(/\[\[symbol\]\]\s*\n/).slice(1).map((block) => ({
    file,
    name: parseField(block, 'ts'),
    kind: parseField(block, 'kind'),
    status: parseField(block, 'status'),
    note: parseField(block, 'note'),
  }));
  symbols.push(...rows);
  collectCallableDeclarations(ts, file, source, addDeclaration);
}

const findDeclaration = new Map();
for (const declaration of declarations) {
  for (const key of [declaration.key, declaration.key.split('.').at(-1)]) {
    const matches = findDeclaration.get(key) ?? [];
    matches.push(declaration);
    findDeclaration.set(key, matches);
  }
}
const portedCallables = symbols.filter((row) => row.status === 'ported' && callableKinds.has(row.kind));
const rowKey = (row) => `${row.file}::${row.name}`;
assert.equal(reviewedRoots.length, expectedAuditedCallableRows, 'reviewed callable corpus total');
assert.equal(new Set(reviewedRoots.map(rowKey)).size, expectedAuditedCallableRows, 'reviewed callable identities are unique');
const reviewedKeys = new Set(reviewedRoots.map(rowKey));
assert(portedCallables.every((row) => reviewedKeys.has(rowKey(row))), 'every ported callable has notification review coverage');
const roots = reviewedRoots.map((reviewed) => {
  const row = symbols.find((item) => rowKey(item) === rowKey(reviewed));
  assert(row, `reviewed callable remains in manifest: ${reviewed.name}`);
  assert.equal(row.kind, reviewed.kind, `reviewed callable kind: ${reviewed.name}`);
  assert(callableKinds.has(row.kind), `reviewed row is callable: ${reviewed.name}`);
  assert.equal(row.status, reviewed.review === 'ported' ? 'ported' : 'divergent', `reviewed callable status: ${reviewed.name}`);
  if (reviewed.review === 'crypto-choice') {
    assert(reviewedCryptoChoices.has(rowKey(row)), `only reviewed crypto roots use crypto-choice: ${reviewed.name}`);
    assert(row.note.startsWith('CHOSEN: MANIFEST-SPEC.md#closed-typed-output-boundary.'), `reviewed crypto choice: ${reviewed.name}`);
  } else if (reviewed.review !== 'ported') {
    assert.equal(reviewed.review, 'notification-finding', 'only explicit notification findings extend the ported cohort');
    assert(row.note.startsWith('FINDING: Coinbase notification audit.'), `reviewed notification finding: ${reviewed.name}`);
    assert(row.note.includes('NOTIFICATION_REACHABILITY_REVIEW.md'), `notification finding review: ${reviewed.name}`);
  }
  const declaration = declarations.find((item) => item.file === row.file && item.key === canonicalSymbol(row.name));
  return { row, declaration, reviewed };
});
assert(roots.every((item) => item.declaration), 'every reviewed callable resolves to a source declaration');
for (const { row, declaration, reviewed } of roots) {
  assert.equal(declaration.kind, reviewed.declarationKind, `reviewed declaration shape: ${row.name}`);
}
const reviewedDeclarationKeys = new Set(roots.map((item) => `${item.row.file}::${canonicalSymbol(item.row.name)}`));
const matchedSourceDeclarations = declarations.filter((item) => reviewedDeclarationKeys.has(`${item.file}::${item.key}`)).length;
assert.equal(matchedSourceDeclarations, expectedAuditedCallableRows, 'source declaration census matches reviewed callable rows');

const directDeclarations = new Set(directWrites.map((item) => item.declaration));
const reachable = [];
for (const { row, declaration } of roots) {
  const queue = [[declaration, [row.name]]];
  const visited = new Set();
  while (queue.length) {
    const [current, chain] = queue.shift();
    if (visited.has(current)) continue;
    visited.add(current);
    if (directDeclarations.has(current)) reachable.push(chain);
    for (const call of current.calls) {
      for (const target of findDeclaration.get(call) ?? []) {
        if (!visited.has(target)) queue.push([target, [...chain, target.key]]);
      }
    }
  }
}

const rustHealth = sourceText(new URL('sdk-rs/safe-cex-venue-coinbase/src/private_health.rs', root));
for (const [name, error] of [
  ['mark_private_user_stream_message_received', 'StoreMutationError'],
  ['mark_private_user_stream_degraded', 'PortfolioFailure'],
  ['mark_private_user_stream_auth_failed', 'PortfolioFailure'],
]) {
  const body = rustHealth.split(`pub fn ${name}(`)[1]?.split(/\n    pub fn /)[0];
  assert(body, `${name} must retain its health implementation`);
  assert.match(
    body,
    new RegExp(`^[\\s\\S]*?\\n    \\)\\s*->\\s*Result<\\(\\), ${error}>`),
    `${name} must expose the fallible Store contract`,
  );
  if (error === 'PortfolioFailure') {
    for (const callback of ['log', 'emit_warning']) {
      assert.match(body, new RegExp(`${callback}: impl FnMut\\([^\\n]*\\) -> Result<\\(\\), PortfolioFailure>`),
        `${name} must retain fallible ${callback} capability`);
    }
  }
}
assert.match(rustHealth, /update_socket_health\(&Value::Object\(update\)\.into\(\)\)\s*\.map_err\(\|error\| Rc::new\(PortfolioThrown::from\(error\)\)\)\?;/,
  'health native origin bridge retains StoreMutationError');
assert.match(rustHealth, /log\(reason, "warning"\)\?;/, 'health log failure must propagate');
assert.match(rustHealth, /emit_warning\(reason\)\?;/, 'health warning failure must propagate');
const rustPrivateSnapshot = sourceText(new URL('sdk-rs/safe-cex-venue-coinbase/src/private_order_snapshot.rs', root));
assert.match(
  rustPrivateSnapshot,
  /pub fn apply_order_snapshot_batch\([\s\S]*?\)\s*->\s*Result<\(\), StoreMutationError>/,
  'apply_order_snapshot_batch must expose the fallible Store contract',
);
const rustNativeFailure = sourceText(new URL('sdk-rs/safe-cex-venue-coinbase/src/portfolios_balances.rs', root));
assert.match(rustNativeFailure, /pub type PortfolioFailure = Rc<PortfolioThrown>;/,
  'health and snapshot failures retain the native PortfolioThrown carrier');
assert.match(
  rustNativeFailure,
  /impl From<safe_cex_store::StoreMutationError> for PortfolioThrown[\s\S]*?store_mutation: Some\(error\)/,
  'snapshot native origin bridge retains StoreMutationError',
);
for (const name of reviewedNotificationRows) {
  const row = symbols.find((item) => canonicalSymbol(item.name) === name);
  assert(row, `fallible Store handler has no manifest row: ${name}`);
  assert.equal(row.status, 'ported', `fallible Store handler status: ${name}`);
  assert(row.note.includes('StoreMutationError'), `fallible Store handler note: ${name}`);
}

const writeRows = directWrites.map(({ declaration, call }) => {
  const row = symbols.find((item) => canonicalSymbol(item.name) === declaration.key);
  assert(row, `direct Store writer has no manifest row: ${declaration.key}`);
  if (reviewedNotificationRows.has(canonicalSymbol(row.name))) {
    assert.equal(row.status, 'ported', `fallible Store handler status: ${row.name}`);
    assert(row.note.includes('StoreMutationError'), `fallible Store handler note: ${row.name}`);
  } else {
    assert(['pending', 'divergent'].includes(row.status), `Store writer cannot claim notification parity: ${row.name}`);
  }
  if (row.status === 'divergent') {
    assert(row.note.startsWith('FINDING:'), `divergent Store writer needs a FINDING note: ${row.name}`);
  }
  return { row: row.name, call };
});
const directCallInventory = directWrites.map(({ declaration, call, syntax }) => ({
  file: declaration.file,
  row: declaration.key,
  call,
  sourceSha256: createHash('sha256').update(syntax).digest('hex'),
}));
{
  assert.equal(directCallInventory.length, 13, 'reviewed direct Store call floor');
  assert.deepEqual(directCallInventory, review.directStoreCalls, 'every direct Store call matches its reviewed source site');
}
const notificationFindings = new Set(roots.filter((item) => item.reviewed.review === 'notification-finding').map((item) => canonicalSymbol(item.row.name)));
const findingReachable = reachable.filter((chain) => notificationFindings.has(canonicalSymbol(chain[0])));
for (const name of notificationFindings) {
  assert(findingReachable.some((chain) => canonicalSymbol(chain[0]) === name), `notification finding still reaches a Store writer: ${name}`);
}
const unhandledReachable = reachable.filter((chain) => !notificationFindings.has(canonicalSymbol(chain[0])) && !reviewedNotificationRows.has(canonicalSymbol(chain[0])));
assert.equal(unhandledReachable.length, 0, 'ported Coinbase call graph reaches an unhandled Store notification');
console.log(JSON.stringify({
  portedCallableRows: portedCallables.length,
  auditedCallableRows: roots.length,
  reviewedCryptoChoiceRoots: roots.filter((item) => item.reviewed.review === 'crypto-choice').map((item) => item.row.name),
  directCallInventory,
  matchedSourceDeclarations,
  injectedCallableProperties: roots.filter((item) => item.declaration.kind === 'injected-function-property').map((item) => item.row.name),
  reviewedNotificationFindingRoots: [...notificationFindings],
  reviewedNotificationFindingChains: findingReachable,
  reachableStoreNotificationChains: unhandledReachable,
  directStoreNotificationCalls: writeRows,
  distinctWriterRows: [...new Set(writeRows.map((item) => item.row))],
  divergentWriterRows: [...new Set(writeRows.filter((item) => symbols.find((row) => canonicalSymbol(row.name) === item.row)?.status === 'divergent').map((item) => item.row))],
}, null, 2));
