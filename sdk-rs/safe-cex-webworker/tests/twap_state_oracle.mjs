// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import {records as identities} from './task_identity_oracle.mjs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const entry = require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH ? process.env.SAFE_CEX_WORKER_LODASH_PATH + '/lodash.js' : 'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'), 'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf', 'installed lodash fingerprint');
const lodash = require(entry);
assert.equal(lodash.VERSION, '4.18.1');
let draws = [], used = 0;
const math = Object.create(Math);
math.random = () => draws[used++ % draws.length];
const random = lodash.runInContext({ Math: math }).random;
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
function source(path) {
  let text = fs.readFileSync(new URL(path, root), 'utf8');
  if(process.argv.includes('--mutate-market-copy') && path==='webworker/twap/TwapInstance.ts') {
    const marker='createNewState(): TwapState {'; assert.ok(text.includes(marker));
    text=text.replace(marker,marker+' [this.options.activeMarket.id,this.options.activeMarket.base]=[this.options.activeMarket.base,this.options.activeMarket.id];');
  }
  const ast = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  return ast.statements.filter(s => !ts.isImportDeclaration(s)).map(s => s.getText(ast)).join('\n').replace(/\bexport /g, '');
}
const context = vm.createContext({ random, Date: { now: () => 123456789 } });
vm.runInContext(ts.transpileModule(source('utils/safe-math.ts') + '\n' + source('webworker/twap/TwapStatus.ts') + '\n' + source('webworker/twap/TwapInstance.ts') + '\nglobalThis.Instance=TwapInstance;', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
const bytes = new DataView(new ArrayBuffer(8));
function bits(n) { if (Number.isNaN(n)) return 'NaN'; bytes.setFloat64(0, n); return bytes.getBigUint64(0).toString(16).padStart(16, '0'); }
const market = {id:'BTC',symbol:' BTC🚀\t',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
function options(size, ordersCount, randomnessPercentage, origin) { return { accountName:' account🚀\t', activeMarket:structuredClone(market), side:'buy', size, ordersCount, randomnessPercentage, origin }; }
function state(value) {
  const result = { ...value, ordersSizes: value.ordersSizes.map(bits) };
  for (const key of ['size','ordersCount','ordersExecuted','nextOrderAt','createdAt']) result[key] = bits(value[key]);
  return result;
}
const rows = [];
const counts = [0,-0,-2,-1,0.5,1,1.5,2,3,8,31,NaN,Infinity,-Infinity,8589934592];
const sizes = [0,-0,1,-1,0.01,1/3,Number.MAX_VALUE,NaN,Infinity];
const percentages = [0,1,-1,12.5,100,NaN,Infinity,-Infinity];
const streams = [[0],[0.1,0.7],[0.9999999999999999],[1e-7,0.25,0.5]];
for (const count of counts) for (const size of sizes) for (const pct of percentages) for (const stream of streams) {
  draws = stream; used = 0;
  const opts = options(size,count,pct, [undefined,'cli','webhook','automation','web'][rows.length % 5]);
  opts.side = rows.length % 3 ? 'sell' : 'buy';
  let result;
  try {
    const value = context.Instance.prototype.createNewState.call({ options: opts });
    result = { state:state(value), nonIndex:Object.fromEntries(Object.getOwnPropertyNames(value.ordersSizes).filter(k => k !== 'length' && !/^(0|[1-9][0-9]*)$/.test(k)).map(k => [k,bits(value.ordersSizes[k])])) };
  } catch (error) { result = { error: { name:error.name, message:error.message } }; }
  rows.push({ count:bits(count), size:bits(size), pct:bits(pct), draws:stream.map(bits), origin:opts.origin, side:opts.side, result, used });
}
draws = [0]; used = 0;
const opts = options(1,2,0,undefined);
const value = context.Instance.prototype.createNewState.call({options:opts});
const alias = { identical: value.activeMarket === opts.activeMarket };
opts.activeMarket.symbol = 'via-options'; alias.stateAfterOptions = value.activeMarket.symbol;
value.activeMarket.base = 'via-state'; alias.optionsAfterState = opts.activeMarket.base;
const loneOpts = options(1,1,0,undefined); loneOpts.accountName = JSON.parse('"\\ud800"');
const lone = context.Instance.prototype.createNewState.call({options:loneOpts});
const marketIdentity=identities.TwapOptions.cases.filter(c=>c.path.startsWith('/activeMarket/')).map(c=>{
  draws=[0];used=0;
  const opts={...options(1,2,0,undefined),activeMarket:structuredClone(c.input.activeMarket)};
  const input=structuredClone(opts.activeMarket);
  const result=context.Instance.prototype.createNewState.call({options:opts});
  const expected=structuredClone(result.activeMarket);
  assert.deepEqual(expected,input,'SOURCE-TWAP-MARKET copy identity');
  return {path:c.path,input,expected};
});
process.stdout.write(JSON.stringify({ rows, alias, marketIdentity, loneSurrogate:{ codeUnit:lone.accountName.charCodeAt(0), json:JSON.stringify(lone) } }));
