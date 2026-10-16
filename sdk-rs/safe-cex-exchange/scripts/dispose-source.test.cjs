const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {test} = require('node:test');
const {DisposeSources, hasSuperDispose} = require('./dispose-source.cjs');

const root = path.resolve(__dirname, '../../..');
const sources = new DisposeSources(root);
const review = JSON.parse(fs.readFileSync(path.join(__dirname, '../tests/override-base-delegations.json'), 'utf8'));
const rows = review.delegations.filter(row => /hook dispose\b|BaseExchange::dispose\b/.test(row.finding));
for (const row of rows) {
  test(`actual dispose source classification: ${row.file}`, () => {
    sources.validate(row);
  });
}

const base = rows.find(row => row.kind === 'base-implementation');
const venue = rows.find(row => row.kind === 'source-super');
const ownNoSuper = sources.describe('packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts', 'BackpackExchange');
const socketNoSuper = sources.describe('packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts', 'BackpackPrivateWebSocket');

test('a source-super label cannot turn an own no-super body into base cleanup', () => {
  const mutant = {...venue, file: 'safe-cex-venue-backpack/src/adapter.rs', dispose_source: [ownNoSuper]};
  assert.throws(() => sources.validate(mutant), /dispose base cleanup has no source super call/);
});
test('a socket receiver type cannot excuse its own no-super override', () => {
  const mutant = {...venue, file: 'safe-cex-venue-backpack/src/private_adapter.rs', kind: 'socket-receiver', dispose_source: [socketNoSuper]};
  assert.throws(() => sources.validate(mutant), /dispose base cleanup has no source super call/);
});
test('chosen-base cannot hide a dispose source mismatch', () => {
  const mutant = {...venue, file: 'safe-cex-venue-backpack/src/adapter.rs', kind: 'chosen-base', dispose_source: [ownNoSuper]};
  assert.throws(() => sources.validate(mutant), /dispose base cleanup has no source super call/);
});
test('disposal classification cannot omit its TS method citation', () => {
  const mutant = {...venue};
  delete mutant.dispose_source;
  assert.throws(() => sources.validate(mutant), /dispose classification needs TS source lines/);
});
test('a stale super-call assertion is rejected against the actual AST', () => {
  const mutant = structuredClone(venue);
  mutant.dispose_source[0].super_call = false;
  assert.throws(() => sources.validate(mutant), /dispose source claim changed/);
});
test('source line spans belong to the named method', () => {
  const mutant = structuredClone(venue);
  mutant.dispose_source[0].start_line += 1;
  assert.throws(() => sources.validate(mutant), /dispose source claim changed/);
});
test('inherited dispose must resolve to its defining base body', () => {
  const row = rows.find(row => row.kind === 'socket-receiver' && row.dispose_source.some(s => !s.own));
  assert(row, 'at least one actual inherited socket disposal');
  const mutant = structuredClone(row);
  mutant.dispose_source.find(s => !s.own).definition_class = 'InventedDisposeBase';
  assert.throws(() => sources.validate(mutant), /dispose source claim changed/);
});
test('the shared base bridge cannot cite a different venue super call', () => {
  const mutant = {...base, dispose_source: venue.dispose_source};
  assert.throws(() => sources.validate(mutant), /base dispose bridge must cite the BaseExchange implementation/);
});
test('a venue cannot borrow another venue super call as evidence', () => {
  const mutant = {...venue, file: 'safe-cex-venue-backpack/src/adapter.rs'};
  assert.throws(() => sources.validate(mutant), /dispose citation must belong to the same venue/);
});
test('comments, strings and a nested class cannot supply the outer method super call', () => {
  const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
  const source = ts.createSourceFile('control.ts', `class Owner extends Base {
    dispose() {
      /* super.dispose(); */
      const text = 'super.dispose()';
      class Other extends Base { dispose() { super.dispose(); } }
    }
    realDispose() { super.dispose(); }
  }`, ts.ScriptTarget.Latest, true);
  const members = source.statements[0].members;
  assert.equal(hasSuperDispose(members[0].body), false);
  assert.equal(hasSuperDispose(members[1].body), true);
});
