// Fixed native field/shape expectations corroborated by actual TS declarations.
const assert = require('node:assert/strict');
const { schemas, accepts } = require('./oracle.cjs');
const selected = [
  'AsterDEXV3AuthContext', 'AsterDEXAccountResponseAssetsItem',
  'AsterDEXAccountResponsePositionsItem', 'AsterDEXAccountResponse',
  'AsterDEXPositionModeResponse', 'AsterDEXOpenInterestResponse',
  'AsterDEXDepthResponse', 'AsterDEXTradeResponse', 'AsterDEXUserTradeResponse',
  'AsterDEXStreamTicker', 'AsterDEXStreamDepth', 'AsterDEXStreamTrade',
  'AsterDEXStreamMarkPrice', 'AsterDEXAccountUpdateABItem',
  'AsterDEXAccountUpdateAPItem', 'AsterDEXAccountUpdateA', 'AsterDEXAccountUpdate',
  'AsterDEXOrderUpdateO', 'AsterDEXOrderUpdate', 'AsterDEXListenKeyResponse',
];
function numbers(value) {
  if (typeof value === 'number') return [value];
  if (Array.isArray(value)) return value.flatMap(numbers);
  if (value && typeof value === 'object') return Object.values(value).flatMap(numbers);
  return [];
}
async function main() {
  assert.equal(process.argv.length, 2, 'record source gate takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  assert.equal(selected.length, 20);
  let count = 0;
  for (const name of selected) {
    const schema = schemas[name];
    const contract = contracts[`safe_cex_venue_asterdex::types::${name}`];
    assert.equal(schema?.kind, 'object', name);
    assert.ok(contract?.nativeObservation, name);
    assert.deepEqual(Object.keys(contract.sample), schema.fields.map(field => field.name),
      `${name}: independently reviewed sample covers every source field in declaration order`);
    assert.ok(accepts(schema, contract.sample), `${name}: complete object is source-valid`);
    const complete = contract.nativeObservation.cases.filter(row => row.label === 'complete-serde-order-array');
    assert.equal(complete.length, 1, name);
    assert.deepEqual(JSON.parse(complete[0].input), Object.values(contract.sample),
      `${name}: complete valid array has every field in reviewed declaration order`);
    assert.equal(complete[0].output.ok, false, name);
    for (const row of contract.nativeObservation.cases) {
      const value = JSON.parse(row.input);
      assert.equal(accepts(schema, value), row.output.ok,
        `${name}: actual source shape for ${row.label}: ${row.input}`);
      if (row.label === 'full-right-decimal-or-negative-zero') {
        assert.ok(numbers(value).every(number => Object.is(number, -0)),
          `${name}: raw JSON negative-zero fixture must retain JS Number sign`);
      }
      count++;
    }
  }
  assert.equal(count, 707);
  console.log(`Aster record source schemas: 20 owners / ${count} independently authored field/shape observations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
