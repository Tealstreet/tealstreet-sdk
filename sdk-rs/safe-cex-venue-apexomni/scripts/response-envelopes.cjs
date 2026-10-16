const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const filename = path.join(__dirname, 'exchange.cjs');
let source = fs.readFileSync(filename, 'utf8');
const driver = 'main().catch(error=>{console.error(error);process.exitCode=1;});';
assert.equal(source.split(driver).length, 2, 'APEX-ENVELOPE maintained driver');
source = source.replace(driver, () => String.raw`
(async () => {
 const rows = [];
 const cases = [
  ['getAccount', 'null-root', null],
  ['getAccount', 'undefined-root', undefined],
  ['getAccount', 'null-data', {data:null}],
  ['getAccount', 'missing-data', {}],
  ['getAccount', 'false-data', {data:false}],
  ['getAccount', 'numeric-id', {data:{id:17}}],
  ['getAccount', 'scalar-id', {data:{id:'é😀'}}],
  ['fetchPositions', 'null-root', null],
  ['fetchPositions', 'undefined-root', undefined],
  ['fetchPositions', 'null-data', {data:null}],
  ['fetchPositions', 'missing-data', {}],
  ['fetchPositions', 'missing-positions', {data:{}}],
  ['fetchPositions', 'null-positions', {data:{positions:null}}],
  ['fetchPositions', 'empty-positions', {data:{positions:[]}}],
  ['fetchPositions', 'numeric-positions', {data:{positions:17}}],
  ['fetchPositions', 'string-positions', {data:{positions:'x'}}],
  ['fetchPositions', 'object-positions', {data:{positions:{}}}],
  ['fetchOrders', 'null-root', null],
  ['fetchOrders', 'undefined-root', undefined],
  ['fetchOrders', 'null-orders', {data:null}],
  ['fetchOrders', 'numeric-orders', {data:17}],
  ['fetchOrders', 'string-orders', {data:'x'}],
  ['fetchOrders', 'object-orders', {data:{}}],
  ['fetchOrders', 'null-after-valid', {data:[order,null]}],
  ['fetchOrders', 'null-after-missing-market', {data:[{...order,symbol:'MISSING'},null]}],
  ['fetchOrders', 'null-after-bad-type', {data:[{...order,type:'constructor',price:{toString:null}},null]}],
  ['fetchOrders', 'numeric-map-key', {data:[{...order,id:17}]}],
  ['fetchOrders', 'cache-nan', {data:[order]}, NaN],
  ['fetchOrders', 'cache-positive-infinity', {data:[order]}, Infinity],
  ['fetchOrders', 'cache-negative-infinity', {data:[order]}, -Infinity],
  ['fetchOrders', 'cache-boundary', {data:[order]}, clock-10000],
  ['fetchOrders', 'cache-expired', {data:[order]}, clock-10001],
  ['fetchOrders', 'cache-future', {data:[order]}, clock+1],
 ];
 for (const [name,label,payload,cancelTime] of cases) {
  const s = store(); operations = [];
  const venue = new Class('apexomni',{key:'key',secret:'secret'},s);
  venue.recentlyCancelledOrders.set('17',clock-999);
  if (label.startsWith('cache-')) venue.recentlyCancelledOrders.set('order-1',cancelTime);
  operations = [];
  const endpoint = name === 'fetchOrders' ? '/v3/open-orders' : '/v3/account';
  responses = {[endpoint]:payload};
  let result;
  try { result = {ok:true,value:native(await venue[name]())}; }
  catch(error) { result = {ok:false,name:error.name,message:error.message}; }
  rows.push({name,label,payload:native(payload),result,operations:native(operations),zk:native(s.zkLinkConfig),cancelled:native([...venue.recentlyCancelledOrders.entries()].sort(([a],[b])=>a.localeCompare(b))),cancelTime:native(cancelTime)});
 }
 assert.equal(rows.length,33,'APEX-ENVELOPE independent source floor');
 console.log(JSON.stringify({header:{node:process.version,architecture:process.arch},rows}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m = new Module(filename, module);
m.filename = filename;
m.paths = Module._nodeModulePaths(path.dirname(filename));
m._compile(source, filename);
