import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { deserialize } from 'node:v8';
import { createServer } from 'node:http';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
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
writeFileSync(`${output}/bybit.markets.json`, JSON.stringify(native(bybit)));
profiles.push({name:'bybit', count:8110, marketsFile:`${output}/bybit.markets.json`, rawFiles:manifest.filter(x=>x.venue==='bybit').map(x=>x.file)});
for (const venue of ['okx', 'paradex']) {
  const requests = [];
  const rawFiles = venue === 'okx' ? manifest.filter(x=>x.venue==='okx').map(x=>x.file) : [`${root}/sdk-rs/safe-cex-ffi/tests/fixtures/live-public/paradex-markets.json`];
  const routes = venue === 'okx' ? manifest.filter(x=>x.venue==='okx').map(x=>({ route:new URL(x.request.trim().slice(4)),file:x.file })) : [{route:new URL('https://api.prod.paradex.trade/v1/markets'),file:rawFiles[0]}];
  const failures = [];
  const server = createServer((request, response) => {
    const observed = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://fixture');
    requests.push(observed.pathname + observed.search);
    const route = routes.find(x=>x.route.pathname===observed.pathname && x.route.search===observed.search);
    if (!route) {
      failures.push(`unrecorded request ${request.url}`);
      response.writeHead(500); response.end('unrecorded fixture request'); return;
    }
    response.setHeader('Content-Type','application/json');
    response.end(readFileSync(route.file));
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const { DefaultStore } = await import(`${root}/packages/safe-cex/src/store/store.base.ts`);
  const module = await import(`${root}/packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`);
  const Constructor = module[venue==='okx'?'OKXExchange':'ParadexExchange'];
  const exchange = new Constructor(venue,{key:'',secret:'',publicDataOnly:true,hostedMode:true,overrideProxyUrl:`http://127.0.0.1:${server.address().port}`,tuning:{consoleLogging:false}},new DefaultStore());
  const markets = await exchange.fetchMarkets();
  await exchange.dispose();
  server.closeAllConnections(); await new Promise(r=>server.close(r));
  assert.deepEqual(failures, []);
  assert.deepEqual(requests, routes.map(x=>x.route.pathname + x.route.search));
  assert.equal(markets.length, {okx:509,paradex:66}[venue], 'complete recorded market census');
  writeFileSync(`${output}/${venue}.markets.json`,JSON.stringify(native(markets)));
  profiles.push({name:venue,count:markets.length,marketsFile:`${output}/${venue}.markets.json`,rawFiles});
  console.log({venue,markets:markets.length,requests});
}
writeFileSync(`${output}/manifest.json`,JSON.stringify(profiles,null,2));
console.log({bybit:bybit.length,manifest:`${output}/manifest.json`});
