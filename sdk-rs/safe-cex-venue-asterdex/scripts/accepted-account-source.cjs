// Maintained source bodies, with explicit Store/clock capabilities for pure plans.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');
const api = require('./api-source.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const printer = ts.createPrinter();
const names = ['ACCOUNT_CURRENCY', 'handleExecutionReport', 'mirrorCumulativeOrderFill',
  'getOrderUpdateTime', 'parseFiniteNumber', 'parseAssetBalanceUpdates', 'getPositionSide',
  'getAccountUpdateUpnlDeltas', 'handleAccountUpdate'];
const members = names.map(name => {
  const member = cls.members.find(node => node.name?.getText(ast) === name);
  assert(member, 'maintained member missing: ' + name);
  return printer.printNode(ts.EmitHint.Unspecified, member, ast);
});
let clockCalls = 0, clockNow = 0;
const exportsObject = {};
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const typesAst = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = {};
new Function('exports', ts.transpileModule(typesAst.statements.filter(ts.isEnumDeclaration)
  .map(node => printer.printNode(ts.EmitHint.Unspecified, node, typesAst)).join('\n'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText)(enums);
const compiled = ts.transpileModule('export class AsterDEXPrivateWebsocket { ' + members.join('\n') + ' }', {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
new Function('exports', 'OrderStatus', 'PositionSide', 'virtualClock', compiled)(exportsObject,
  enums.OrderStatus, enums.PositionSide,
  { getCurrentTimeMs: () => { clockCalls++; return clockNow; } });
const SourceAccount = exportsObject.AsterDEXPrivateWebsocket;
const bits = n => { if (Number.isNaN(n)) return 'NaN'; const bytes = Buffer.alloc(8); bytes.writeDoubleBE(n); return bytes.toString('hex'); };
const balances = rows => rows.map(row => ({ asset: row.asset, bits: [row.total, row.free, row.used].map(bits) }));
const sourceOwners = ['account_helpers::handle_execution_report', 'account_helpers::mirror_cumulative_order_fill',
  'account_helpers::handle_account_update', 'account_helpers::parse_asset_balance_updates',
  'account_helpers::get_account_update_upnl_deltas', 'api_helpers::generate_v1_signature',
  'api_helpers::generate_aster_dex_code_message_signature'];
function observe(name, input) {
  const owner = new SourceAccount();
  owner.scheduleOrderReconcile = () => {};
  owner.parent = { store: {}, emitter: { emit: () => {} } };
  try {
    switch (name) {
      case 'account_helpers::handle_execution_report': {
        owner.parent.store.orders = input.ids.map(id => ({ id }));
        let result = null;
        owner.parent.store.updateOrder = (row, patch) => {
          const status = typeof patch.status === 'string' ? { kind: 'mapped', value: patch.status }
            : { kind: 'inherited', text: String(patch.status), arity: patch.status.length ?? null };
          if (status.kind === 'inherited') assert.equal(patch.status,
            input.data.X === '__proto__' ? Object.prototype : Object.prototype[input.data.X]);
          result = { id: row.id, status, bits: [patch.filled, patch.remaining].map(bits) };
        };
        owner.handleExecutionReport(input.data);
        return result;
      }
      case 'account_helpers::mirror_cumulative_order_fill': {
        clockCalls = 0; clockNow = input.now;
        let result = null;
        owner.parent.store.updateOrder = (row, patch) => {
          result = { id: row.id, status: patch.status, bits: [patch.filled, patch.remaining, patch.lastUpdate].map(bits) };
        };
        owner.mirrorCumulativeOrderFill(input.data);
        return { value: result, clock: clockCalls };
      }
      case 'account_helpers::parse_asset_balance_updates': {
        const rows = owner.parseAssetBalanceUpdates(input.rows, input.fields);
        return rows === undefined ? null : balances(rows);
      }
      case 'account_helpers::handle_account_update': {
        const effects = [];
        owner.applyBalanceUpdates = (rows, deltas) => {
          assert(deltas instanceof Map && deltas.size === 0);
          effects.push({ kind: 'balances', rows: balances(rows) });
        };
        owner.parent.emitter.emit = (event, data) => {
          assert.equal(event, 'account');
          effects.push({ kind: 'emit', same: data === input });
        };
        owner.handleAccountUpdate(input);
        return effects;
      }
      case 'account_helpers::get_account_update_upnl_deltas': {
        owner.parent.store.positions = input.existing.map(row => ({ ...row, unrealizedPnl: Number(row.previous) }));
        owner.parent.store.balance = { currencies: Object.fromEntries(input.currencies.map(key => [key, {}])) };
        const result = owner.getAccountUpdateUpnlDeltas(input.positions);
        return result === undefined ? null : [...result].map(([key, number]) => [key, bits(number)]);
      }
      case 'api_helpers::generate_v1_signature':
        return { signature: api.generateV1Signature(input.query, input.secret) };
      case 'api_helpers::generate_aster_dex_code_message_signature':
        return { signature: api.generateAsterDEXCodeMessageSignature(input.query, input.key, input.testnet) };
      default: throw Error('unreviewed source owner ' + name);
    }
  } catch (error) {
    // Fixture assertion/dependency errors must never become golden SDK errors.
    if (error instanceof assert.AssertionError || ['ReferenceError', 'SyntaxError'].includes(error.name)) throw error;
    if (name === 'account_helpers::mirror_cumulative_order_fill') return { value: { error: error.name }, clock: clockCalls };
    return { error: error.name };
  }
}
async function main() {
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  let cases = 0;
  for (const name of sourceOwners) {
    const contract = contracts['safe_cex_venue_asterdex::' + name];
    assert.equal(contract?.category, 'decoder', name);
    for (const row of contract.cases) {
      assert.deepEqual(observe(name, structuredClone(row.input)), row.output, name + ': ' + JSON.stringify(row.input));
      cases++;
    }
  }
  console.log('Aster account actual source: 7 owners / ' + cases + ' expectations pass.');
}
module.exports = { observe, sourceOwners };
if (require.main === module) main().catch(error => { console.error(error); process.exitCode = 1; });
