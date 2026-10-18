// Source DTO admission only. Runtime adapters and typed constructors have separate witnesses.
const assert = require('node:assert/strict');
const { schemas, accepts } = require('./oracle.cjs');
const selected = ['AsterDEXMarketResponseFiltersItem', 'AsterDEXMarketResponse',
  'AsterDEXPositionResponse', 'AsterDEXOrderResponse', 'AsterDEXTickerResponse',
  'AsterDEXKlineResponse', 'AsterDEXWebSocketMessage', 'AsterDEXPlaceOrderOpts'];
async function main() {
  assert.equal(process.argv.length, 2, 'alternate source gate takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  let count = 0;
  for (const name of selected) {
    const contract = contracts[`safe_cex_venue_asterdex::types::${name}`];
    const schema = schemas[name];
    assert.ok(schema && contract?.nativeObservation, name);
    if (schema.kind === 'object') assert.deepEqual(Object.keys(contract.sample), schema.fields.map(f => f.name), name);
    else assert.equal(contract.sample.length, schema.items.length, name);
    for (const row of contract.nativeObservation.cases) {
      assert.equal(accepts(schema, JSON.parse(row.input)), row.output.ok,
        `${name}: maintained TS declaration for ${row.label}`);
      count++;
    }
  }
  assert.equal(count, 435);
  console.log(`Aster alternate DTO source: 8 owners / ${count} field and shape expectations pass.`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
