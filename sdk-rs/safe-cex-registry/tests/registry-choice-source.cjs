'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const {root, ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const helperFile = __dirname + '/loaders.cjs';
const helperAst = ts.createSourceFile(helperFile, fs.readFileSync(helperFile, 'utf8'), ts.ScriptTarget.Latest, true);
const helpers = helperAst.statements.filter(node => ts.isFunctionDeclaration(node) && node.name?.text === 'registry');
assert.equal(helpers.length, 1, 'REGISTRY CHOICE maintained source capture helper');
const registry = new Function('fs', 'assert', 'root', 'ts', helpers[0].getText(helperAst) + ';return registry;')(fs, assert, root, ts);
const deadline = setTimeout(() => {
  console.error('REGISTRY CHOICE source did not finish');
  process.exitCode = 1;
}, 5000);
(async () => {
  let cases = 0;
  for (const native of [true, false]) {
    for (const kind of ['undefined', 'null', 'number', 'string', 'object', 'constructor', 'thenable']) {
      let adopted = 0;
      function Capability() {}
      Capability.staticValue = 47;
      Capability.prototype.marker = 53;
      const value = kind === 'undefined' ? undefined : kind === 'null' ? null : kind === 'number' ? 17 : kind === 'string' ? 'raw' : kind === 'object' ? {own: 29} : kind === 'constructor' ? Capability : {then(resolve) {adopted++; resolve(41);}};
      const expected = kind === 'thenable' ? 41 : value;
      const host = path => {
        assert.equal(path, './apexomni/apexomni.exchange', 'REGISTRY CHOICE actual module');
        const namespace = {ApexOmniExchange: value};
        return native ? namespace : Promise.resolve(namespace);
      };
      const entries = registry(native, host);
      assert.equal(Object.keys(entries)[0], 'apexomni', 'REGISTRY CHOICE independent selected entry');
      const actual = await entries.apexomni();
      assert.equal(actual, expected, 'REGISTRY CHOICE source admits ' + kind);
      if (kind === 'constructor') {
        assert.equal(actual.staticValue, 47, 'REGISTRY CHOICE source constructor static field');
        assert.equal(actual.prototype.marker, 53, 'REGISTRY CHOICE source constructor prototype field');
      }
      assert.equal(adopted, kind === 'thenable' ? 1 : 0, 'REGISTRY CHOICE source thenable adoption');
      cases++;
    }
    const entries = registry(native, () => {throw new Error('REGISTRY CHOICE replaced loader must not call host');});
    const replacement = {replacement: true};
    assert.doesNotThrow(() => {entries.apexomni = () => Promise.resolve(replacement);}, 'REGISTRY CHOICE source registry permits replacement');
    entries.added = 17;
    Object.setPrototypeOf(entries, {inherited: 23});
    assert.equal(await entries.apexomni(), replacement, 'REGISTRY CHOICE source mutable loader replacement');
    assert.equal(entries.added, 17, 'REGISTRY CHOICE source added property');
    assert.equal(entries.inherited, 23, 'REGISTRY CHOICE source mutable registry prototype');
    cases++;
  }
  let settle;
  const events = [];
  const namespace = {get ApexOmniExchange() {events.push('export');return function Constructor() {};}};
  const entries = registry(false, path => {
    assert.equal(path, './apexomni/apexomni.exchange');
    events.push('import');
    return new Promise(resolve => {settle = resolve;});
  });
  void entries.apexomni();
  assert.deepEqual(events, ['import'], 'REGISTRY CHOICE source begins before settlement');
  settle(namespace);
  for (let iteration = 0; iteration < 4; iteration++) await Promise.resolve();
  assert.deepEqual(events, ['import', 'export'], 'REGISTRY CHOICE discarded source Promise continues');
  cases++;
  assert.equal(cases, 17, 'REGISTRY CHOICE independent completed source floor');
  clearTimeout(deadline);
  console.log('REGISTRY CHOICE 17 source cases');
})().catch(error => {clearTimeout(deadline);console.error(error);process.exitCode = 1;});
