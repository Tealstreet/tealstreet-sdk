import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { deserialize } from 'node:v8';
import { createServer } from 'node:http';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const argument = (name) => {
  const value = process.argv.find(v => v.startsWith(`--${name}=`))?.slice(name.length + 3);
  if (!value) throw new Error(`missing --${name}=...`);
  return path.resolve(value);
};
const captureManifest = argument('capture-manifest');
const bybitMarkets = argument('bybit-markets');
const output = argument('output');
mkdirSync(output, { recursive: true });
const manifest = JSON.parse(readFileSync(captureManifest));
function native(value) {
  if (value === undefined) return ['u'];
  if (value === null) return ['z'];
  if (typeof value === 'number') { const b = Buffer.alloc(8); b.writeDoubleBE(value); return ['f', b.toString('hex')]; }
  if (typeof value === 'string') return ['s', value];
  if (typeof value === 'boolean') return ['b', value];
  if (Array.isArray(value)) return ['a', value.map(native)];
  return ['o', Object.entries(value).map(([key, value]) => [key, native(value)])];
}
const profiles = [];
const bybit = deserialize(readFileSync(bybitMarkets));
if (bybit.length !== 8110) throw new Error(`recorded Bybit count ${bybit.length}, expected 8110`);
const cpu = () => { const time = process.cpuUsage(); return (time.user + time.system) / 1000; };
function routeKey(url) {
  const copy = new URL(url);
  copy.searchParams.delete('requestId');
  copy.searchParams.sort();
  return copy.pathname + copy.search;
}
for (const venue of ['bybit', 'okx', 'paradex']) {
  const requests = [];
  const rawFiles = venue !== 'paradex' ? manifest.filter(x=>x.venue===venue).map(x=>x.file) : [`${root}/sdk-rs/safe-cex-ffi/tests/fixtures/live-public/paradex-markets.json`];
  const routes = venue !== 'paradex' ? manifest.filter(x=>x.venue===venue).map(x=>({ route:new URL(x.request.trim().slice(4)),file:x.file })) : [{route:new URL('https://api.prod.paradex.trade/v1/markets'),file:rawFiles[0]}];
  const bodies = new Map(routes.map(route=>[routeKey(route.route),readFileSync(route.file,'utf8')]));
  const failures = [];
  const server = createServer((request, response) => {
    const observed = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://fixture');
    requests.push(observed.pathname + observed.search);
    const body = bodies.get(routeKey(observed));
    if (body === undefined) {
      failures.push(`unrecorded request ${request.url}`);
      response.writeHead(500); response.end('unrecorded fixture request'); return;
    }
    response.setHeader('Content-Type','application/json');
    response.end(body);
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const { DefaultStore } = await import(`${root}/packages/safe-cex/src/store/store.base.ts`);
  const module = await import(`${root}/packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`);
  const Constructor = module[{bybit:'BybitExchange',okx:'OKXExchange',paradex:'ParadexExchange'}[venue]];
  const exchange = new Constructor(venue,{key:'',secret:'',publicDataOnly:true,hostedMode:true,enableOptionMarkets:true,overrideProxyUrl:`http://127.0.0.1:${server.address().port}`,tuning:{consoleLogging:false}},new DefaultStore());
  const originalParse = JSON.parse;
  const rawBodies = new Set(bodies.values());
  let parseMs=0,parseCpuMs=0,parseCount=0,waiting=0,waitStart,waitCpuStart,waitMs=0,waitCpuMs=0;
  JSON.parse = function(text,...args) {
    if (!rawBodies.has(text)) return originalParse.call(this,text,...args);
    const start=performance.now(), startCpu=cpu();
    try { return originalParse.call(this,text,...args); }
    finally { parseMs+=performance.now()-start; parseCpuMs+=cpu()-startCpu; parseCount++; }
  };
  const get = exchange.xhr.get;
  exchange.xhr.get = function(...args) {
    if (waiting++===0) { waitStart=performance.now(); waitCpuStart=cpu(); }
    return get.apply(this,args).finally(()=>{
      if (--waiting===0) { waitMs+=performance.now()-waitStart; waitCpuMs+=cpu()-waitCpuStart; }
    });
  };
  let markets, timing;
  const start=performance.now(), startCpu=cpu();
  try {
    markets = await exchange.fetchMarkets();
    const fetchMs=performance.now()-start,fetchCpuMs=cpu()-startCpu;
    // GET intervals include real Axios parsing and transport waits.
    // The residual includes mapping, source bookkeeping and log construction.
    timing={fetchMs,fetchCpuMs,parseMs,parseCpuMs,sourceActiveMs:fetchMs-waitMs,sourceActiveCpuMs:fetchCpuMs-waitCpuMs,parseCount};
  } finally { JSON.parse=originalParse; exchange.xhr.get=get; }
  assert.equal(waiting,0,'all measured requests settled before the source result');
  await exchange.dispose();
  server.closeAllConnections(); await new Promise(r=>server.close(r));
  assert.deepEqual(failures, []);
  assert.deepEqual(requests.map(url=>routeKey(new URL(url,'http://fixture'))).sort(), routes.map(x=>routeKey(x.route)).sort());
  assert.equal(parseCount,routes.length,'each recorded response uses the actual JSON parser');
  assert.equal(markets.length, {bybit:8110,okx:509,paradex:66}[venue], 'complete recorded market census');
  if (venue==='bybit') assert.deepEqual(native(markets),native(bybit),'complete captured Bybit output');
  writeFileSync(`${output}/${venue}.markets.json`,JSON.stringify(native(markets)));
  profiles.push({name:venue,count:markets.length,marketsFile:`${output}/${venue}.markets.json`,rawFiles,sourceCpuMs:timing.parseCpuMs+timing.sourceActiveCpuMs,sourceWallMs:timing.parseMs+timing.sourceActiveMs});
  console.log('SOURCE_PAYLOAD_RESULT',JSON.stringify({venue,markets:markets.length,...timing,parseMapMs:timing.parseMs+timing.sourceActiveMs,parseMapCpuMs:timing.parseCpuMs+timing.sourceActiveCpuMs,requests}));
}
writeFileSync(`${output}/manifest.json`,JSON.stringify(profiles,null,2));
console.log({bybit:bybit.length,manifest:`${output}/manifest.json`});
