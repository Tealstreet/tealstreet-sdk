// Independent oracle: the same @msgpack/msgpack package used by TS.
const { oracleRequire, verifyPackage } = require('../oracle-packages.cjs');
verifyPackage('@msgpack/msgpack');
const { encode } = oracleRequire('@msgpack/msgpack');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const inputs = [
  ['two-key', '{"z":1,"a":2}'],
  ['reversed', '{"a":2,"z":1}'],
  ['signature-last', '{"z":1,"a":2,"signature":"fixture"}'],
  ['nested', '{"z":{"b":true,"a":null},"a":[1,-33,128,"雪\\u0000"]}'],
  ['numeric-boundaries', '[0,127,128,255,256,65535,65536,-1,-32,-33,-128,-129,1.5]'],
  ['integer-number', '[1.0,-0.0,1e3,9007199254740991,9007199254740992]'],
  ['index-keys', '{"z":0,"10":10,"2":2,"01":1,"4294967295":3}'],
  ['overflow', '[1e400,-1e400,1e-400]'],
  ['empty', '{"z":[],"a":{},"empty":""}'],
];
const cases = inputs.map(([name, raw]) => ({ name, raw, hex: Buffer.from(encode(JSON.parse(raw))).toString('hex') }));
const fixture = path.join(__dirname, 'vectors.json');
const result = JSON.stringify({ oracle: '@msgpack/msgpack 3.1.3', cases }, null, 2) + '\n';
if (process.argv.includes('--capture')) fs.writeFileSync(fixture,result,{flag:'wx'});
else assert.equal(result, fs.readFileSync(fixture,'utf8'), 'MessagePack oracle drift');
