// Interfaces erase at runtime. Execute the maintained complete module and use
// its compiler-resolved schemas, then observe actual JSON.parse objects.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const reference = JSON.parse(execFileSync(process.execPath, [path.join(__dirname, 'types_oracle.cjs')], {encoding:'utf8'}));
const rows = JSON.parse(fs.readFileSync(path.join(__dirname, 'dto_fixtures.json'), 'utf8'));
const names = ['CryptocomInstrument','CryptocomBook','CryptocomTicker','CryptocomTrade','CryptocomCandlestick','CryptocomPositionBalance','CryptocomBalanceSummary','CryptocomOrder','CryptocomAccountsResponse','CryptocomTradeFill','CryptocomError'];
const extraKeys = ['1','2','4294967294','z','__middle','01','4294967295','__duplicate','__overflow','__negativeZero','__underflow','__numberObject','__nested','__proto__','additional_fields','input_keys'];
assert.equal(rows.length, 11, 'DTO-source-independent-count');
assert.deepEqual(rows.map(row=>row.name), names, 'DTO-source-independent-names');
function bits(number) {const buffer=Buffer.alloc(8);buffer.writeDoubleBE(number);return buffer.toString('hex');}
function observe(value, row) {
  const extras=Object.keys(value).filter(key=>!row.known.includes(key));
  assert.deepEqual(extras, extraKeys, `DTO-source-extra-keys:${row.name}`);
  assert.deepEqual(Object.keys(value), row.keys, 'DTO-source-full-keys');
  assert.deepEqual([bits(value.__overflow),bits(value.__negativeZero),bits(value.__underflow)], ['7ff0000000000000','8000000000000000','8000000000000000']);
  assert.equal(value.__duplicate,'last');
  assert.deepEqual(Object.keys(value.__nested), ['1','2','z','01','infinite','zero']);
  assert.equal(bits(value.__nested.infinite),'fff0000000000000');
  assert.equal(bits(value.__nested.zero),'8000000000000000');
  assert.equal(value.__numberObject['$serde_json::private::Number'],'ordinary-object');
  assert.equal(Object.getPrototypeOf(value),Object.prototype);
  assert.equal(Object.hasOwn(value,'__proto__'),true);
  assert.equal(value.__proto__.safe,'own-data');
  assert.equal(value.additional_fields,'source-data');
  assert.deepEqual(value.input_keys,['source-data']);
  return {keys:Object.keys(value),extraCount:extras.length,bits:[bits(value.__overflow),bits(value.__negativeZero),bits(value.__underflow)]};
}
const observations=rows.map(row=>{
  const group=reference.wire[row.name] ?? reference[{CryptocomBook:'book',CryptocomBalanceSummary:'summary',CryptocomAccountsResponse:'accounts'}[row.name]];
  assert.deepEqual(row.known,group.fields.map(([name])=>name),'DTO-source-maintained-schema');
  const value=JSON.parse(row.raw);
  const result=observe(value,row);
  const children=row.name==='CryptocomBook'?value.data:row.name==='CryptocomBalanceSummary'?value.position_balances:row.name==='CryptocomAccountsResponse'?[value.this_account]:[];
  const count=['CryptocomBook','CryptocomBalanceSummary'].includes(row.name)?2:row.name==='CryptocomAccountsResponse'?1:0;
  assert.equal(children.length,count,'DTO-source-independent-child-count');
  result.children=children.map((child,index)=>observe(child,row.nested[index]));
  return result;
});
// Enumeration floor: source row10 CryptocomError; five nested record instances.
process.stdout.write(JSON.stringify({names,rows:observations}));
