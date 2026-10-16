const fs = require('node:fs'), assert = require('node:assert/strict');
const {root,ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const source = fs.readFileSync(root+'/packages/safe-cex/src/utils/wsDiagnostics.ts','utf8');
const exportsObject = {};
new Function('exports',ts.transpileModule(source+'\nexport {redactJsonValue};',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exportsObject);
let cases = 0;
for (const prototype of ['primitive',null,{hidden:1},[]]) {
  const input = JSON.parse(`{"__proto__":${JSON.stringify(prototype)},"constructor":"own","toString":"own","ok":2}`);
  for (const depth of [0,12,13]) {
    const result = exportsObject.redactJsonValue(input,depth,false);
    assert.equal(Object.keys(result).length,depth>12?4:3,'source own-key count');
    assert.equal(Object.hasOwn(result,'__proto__'),depth>12,'source local setter own-key');
    assert.equal(result.ok,2);assert.equal(result.constructor,'own');cases++;
  }
  assert.equal(exportsObject.redactWsDiagnosticPayload(JSON.stringify(input)), '{"constructor":"own","toString":"own","ok":2}');
}
assert.equal(cases,12,'independent source value/depth floor');
