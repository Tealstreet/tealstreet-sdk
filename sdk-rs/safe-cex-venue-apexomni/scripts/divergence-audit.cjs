const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency = createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni');
function declarations(file, names) {
  const source = fs.readFileSync(path.join(root, file), 'utf8');
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const wanted = new Set(names);
  const selected = [];
  for (const node of tree.statements) {
    const declared = ts.isFunctionDeclaration(node) ? [node.name?.text] : ts.isVariableStatement(node) ? node.declarationList.declarations.map(d => d.name.getText(tree)) : [];
    if (declared.some(name => wanted.has(name))) {
      selected.push(node.getText(tree));
      for (const name of declared) wanted.delete(name);
    }
  }
  assert.equal(wanted.size, 0, 'APEX-AUDIT actual declaration closure');
  return selected.join('\n');
}
let source = declarations('apexomni.exchange.ts', ['d8', 'generateApeXRandomClientId']) + '\n' + declarations('apexomni.api.ts', ['serializeApexPayloadLegacy']);
const control = process.env.SAFE_CEX_APEX_AUDIT_SOURCE_CONTROL;
assert(!control || ['client-id', 'query'].includes(control), 'APEX-AUDIT known control');
if (control) {
  const from = control === 'client-id' ? '${accountId}' : 'qs.stringify(payload,';
  const to = control === 'client-id' ? '${"replacement"}' : 'qs.stringify({},';
  assert.equal(source.split(from).length, 2, 'APEX-AUDIT control selector');
  source = source.replace(from, to);
}
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const math = Object.create(Math);
math.random = () => 0;
const api = new vm.Script(compiled + '\n({generateApeXRandomClientId, serializeApexPayloadLegacy})').runInNewContext({ exports: {}, Date: { now: () => 0 }, Math: math, qs: dependency('qs') });
const lone = JSON.parse('"\\ud800"');
assert.equal(lone.length, 1, 'APEX-AUDIT materialized length');
assert.equal(lone.charCodeAt(0), 0xd800, 'APEX-AUDIT materialized unit');
const clientId = api.generateApeXRandomClientId(lone);
assert.equal(clientId.charCodeAt(9), 0xd800, 'APEX-AUDIT returned client-id unit');
const query = api.serializeApexPayloadLegacy({ x: lone });
assert.equal(query, 'x=%F0%90%80%80', 'APEX-AUDIT transmitted query');
console.log(JSON.stringify({ node: process.version, architecture: process.arch, inputUnits: [lone.charCodeAt(0)], clientIdUnit9: clientId.charCodeAt(9), query, stoppedAfter: 2 }));
