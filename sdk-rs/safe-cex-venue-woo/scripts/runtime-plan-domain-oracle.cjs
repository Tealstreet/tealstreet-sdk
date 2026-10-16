// Actual source helpers plus explicitly typed declaration fixtures; no selector body edits.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function load(name, imports = {}, extra = '') {
  const exports = {};
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/woo', name), 'utf8');
  vm.runInNewContext(ts.transpileModule(source + extra, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText, { exports, require: key => {
    assert.ok(Object.hasOwn(imports, key), key); return imports[key];
  } });
  return exports;
}
const contracts = load('woo.v3.contracts.ts');
const extra = `
export const declarationDomainFixtures = () => {
  const lone = JSON.parse('"\\\\ud800"') as string;
  const legacyTarget = target('legacy', 'GET', lone) as WooV3LegacyRouteSelection['authoritative'];
  const v3Target = target('v3', 'GET', lone) as WooV3AlreadyV3RouteSelection['authoritative'];
  const legacy: WooV3LegacyRouteSelection = Object.freeze({status:'ready',routeId:'markets',requestedMode:'legacy',gate:'publicReadParity',authoritative:legacyTarget,storeAuthority:'legacy-only'});
  const shadow: WooV3ShadowReadSelection = Object.freeze({status:'ready',routeId:'markets',requestedMode:'shadow-v3',gate:'publicReadParity',authoritative:legacyTarget,observer:v3Target,storeAuthority:'legacy-only',mutationDispatch:'none'});
  const v3: WooV3V3RouteSelection = Object.freeze({status:'ready',routeId:'markets',requestedMode:'v3',gate:'publicReadParity',authoritative:v3Target,storeAuthority:'v3-after-gate'});
  const already: WooV3AlreadyV3RouteSelection = Object.freeze({status:'ready',routeId:'transaction-history',requestedMode:'legacy',gate:'privateReadParity',authoritative:v3Target,storeAuthority:'existing-v3'});
  const invalid = selectWooV3RuntimeRoute('markets', lone) as WooV3InvalidModeSelection;
  const union: readonly WooV3RuntimeRouteSelection[] = [legacy,shadow,v3,already,invalid];
  return {lone,legacyTarget,union};
};
export { target };
`;
const api = load('woo.v3.runtime-selector.ts', {'./woo.v3.contracts': contracts}, extra);
const fixture = api.declarationDomainFixtures();
assert.equal(fixture.lone.length, 1, 'source materialized lone unit length');
const rows = [{name:'target',record:fixture.legacyTarget,slots:['path']},
  ...fixture.union.map((record,index) => ({name:['legacy','shadow','v3','already-v3','invalid-mode'][index],record,
    slots:index===1?['authoritative.path','observer.path']:index===4?['requestedMode']:['authoritative.path']}))];
for (const row of rows) {
  assert.ok(Object.isFrozen(row.record), 'source declaration outer freeze');
  row.units = row.slots.map(slot => {
    const value = slot.split('.').reduce((v,key) => v[key], row.record);
    assert.equal(typeof value, 'string');
    assert.equal(value.length, 1);
    assert.equal(value.charCodeAt(0), 0xd800, 'source declaration returned observable U+D800');
    return value.charCodeAt(0);
  });
  row.wire = JSON.stringify(row.record);
  delete row.record;
}
assert.equal(rows.length,6,'fixed source declaration refusal floor');
const positives = ['ordinary', '😀', String.raw`\ud800`].map(value => {
  const target = api.target('legacy','GET',value);
  assert.equal(target.path, value, 'source positive path exact');
  return JSON.stringify(target.path);
});
process.stdout.write(JSON.stringify({rows,positives,gatesClosed:Object.values(contracts.WOO_V3_MIGRATION_GATES).every(v=>v===false)}));
