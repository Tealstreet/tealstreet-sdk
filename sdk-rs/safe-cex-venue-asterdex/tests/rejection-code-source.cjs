'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { ts, root, values } = require('../scripts/source.cjs');
const file = 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts';
const tree = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
const owner = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'AsterDEXExchange');
const members = ['getFapiPath', 'fetchOrders'].map(name => {
  const member = owner.members.find(n => n.name?.getText(tree) === name);
  assert(member, name);
  return member.getText(tree);
}).join('\n');
const compiled = ts.transpileModule(`export class Source { ${members} }`, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const result = {};
new Function('exports', 'getAsterDEXFapiPath', 'LogSeverity', compiled)(result, values.getAsterDEXFapiPath, { Error: 'error' });
(async () => {
  const codes = ['ERR_NATIVE', 'ERR_λ', 'ERR_\ud800x'];
  for (const code of codes) {
    const source = new result.Source();
    source.apiVersion = 'v1';
    let calls = 0;
    const original = Object.assign(new Error('native Aster rejection'), { code });
    source.primaryAPI = { get: async () => { calls++; throw original; } };
    const logs = [];
    source.log = (...args) => logs.push(args);
    let caught;
    try { await source.fetchOrders(); } catch (error) { caught = error; }
    assert.equal(caught, original);
    assert.equal(caught.code, code);
    assert.equal(calls, 1);
    assert.equal(logs.length, 1);
  }
  process.stdout.write(JSON.stringify(codes.map(code => Array.from({ length: code.length }, (_, i) => code.charCodeAt(i)))));
})().catch(error => { process.stderr.write(error.stack); process.exitCode = 1; });
