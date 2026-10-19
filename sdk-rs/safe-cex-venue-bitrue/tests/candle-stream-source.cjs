'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dayjs = require('dayjs');
dayjs.extend(require('dayjs/plugin/utc'));
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bitrue/bitrue.ws-public.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(ts.isClassDeclaration);
const member = cls.members.find(row => row.name?.getText(tree) === 'mapKline');
assert(member && ts.isArrowFunction(member.initializer));
const code = ts.transpileModule('(' + member.initializer.getText(tree) + ')', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const map = vm.runInNewContext(code, {dayjs});
const cases = [];
for (const ds of ['2024-06-08 01:23:45', '2024-06-08T01:23:45.250', 0, 'not-a-date']) {
  const raw = '{"ds":' + JSON.stringify(ds) + ',"open":11.25,"high":23.75,"low":-0,"close":17.5,"vol":31.125}';
  const input = JSON.parse(raw);
  const candles = map([input]);
  assert.equal(candles.length, 1);
  assert.equal(candles[0].info, input);
  const fields = Object.fromEntries(['timestamp','open','high','low','close','volume'].map(key => {
    const value = candles[0][key], buffer = Buffer.alloc(8);
    buffer.writeDoubleBE(value);
    return [key, Number.isNaN(value) ? 'nan' : buffer.toString('hex')];
  }));
  cases.push({raw, fields, keys: Object.keys(candles[0]), wire: JSON.stringify(candles)});
}
assert.equal(cases.length, 4);
process.stdout.write(JSON.stringify(cases));
