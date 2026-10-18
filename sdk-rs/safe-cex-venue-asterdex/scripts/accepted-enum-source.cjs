// Compare fixed native expectations with actual maintained TS type declarations.
const assert = require('node:assert/strict');
const { schemas, accepts } = require('./oracle.cjs');
const field = (owner, name) => {
  const value = schemas[owner]?.fields.find(item => item.name === name)?.shape;
  assert.ok(value, `${owner}.${name}: actual field schema required`);
  return value;
};
// Use the actual parent field, rather than the oracle's hardcoded nested aliases.
const position = field('AsterDEXPositionModeResponse', 'dualSidePosition');
assert.equal(position.kind, 'union');
assert.equal(position.items.filter(item => item.kind === 'boolean').length, 1);
const positionStrings = { kind: 'union', items: position.items.filter(item => item.kind === 'literal') };
assert.equal(positionStrings.items.length, 2);
const selected = {
  AsterDEXAPIVersion: schemas.AsterDEXAPIVersion,
  AsterDEXFapiVersion: schemas.AsterDEXFapiVersion,
  AsterDEXStpMode: schemas.AsterDEXStpMode,
  AsterDEXPositionModeResponseDualSidePositionString: positionStrings,
  AsterDEXPositionModeResponseDualSidePosition: position,
  AsterDEXStreamTickerE: field('AsterDEXStreamTicker', 'e'),
  AsterDEXStreamDepthE: field('AsterDEXStreamDepth', 'e'),
  AsterDEXStreamTradeE: field('AsterDEXStreamTrade', 'e'),
  AsterDEXStreamMarkPriceE: field('AsterDEXStreamMarkPrice', 'e'),
  AsterDEXAccountUpdateE: field('AsterDEXAccountUpdate', 'e'),
  AsterDEXOrderUpdateE: field('AsterDEXOrderUpdate', 'e'),
};
function tags(schema) {
  if (schema.kind === 'literal') return [schema.value];
  assert.equal(schema.kind, 'union', 'string enum must retain its exact source literal schema');
  return schema.items.flatMap(tags);
}
async function main() {
  assert.equal(process.argv.length, 2, 'enum source gate takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  assert.equal(Object.keys(selected).length, 11);
  let count = 0;
  for (const [name, schema] of Object.entries(selected)) {
    const contract = contracts[`safe_cex_venue_asterdex::types::${name}`];
    assert.ok(contract?.nativeObservation, name);
    if (name === 'AsterDEXPositionModeResponseDualSidePosition') {
      assert.deepEqual(position.items.map(item => item.kind), ['boolean', 'literal', 'literal']);
      assert.deepEqual(positionStrings.items.map(item => item.value), ['true', 'false']);
    } else {
      assert.deepEqual(tags(schema), contract.literals, `${name}: complete independently reviewed tag set`);
      for (const tag of contract.literals) {
        assert.ok(contract.nativeObservation.cases.some(row => {
          const value = JSON.parse(row.input);
          return value && typeof value === 'object' && !Array.isArray(value)
            && Object.keys(value).length === 1 && Object.hasOwn(value, tag)
            && value[tag] === null && row.output.ok === false;
        }), `${name}: each valid tag must refuse its tagged-map form`);
      }
    }
    for (const row of contract.nativeObservation.cases) {
      assert.equal(accepts(schema, JSON.parse(row.input)), row.output.ok,
        `${name}: actual TS schema for ${row.input}`);
      count++;
    }
  }
  assert.equal(count, 173);
  console.log(`Aster enum source schemas: 11 owners / ${count} independently authored shape observations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
