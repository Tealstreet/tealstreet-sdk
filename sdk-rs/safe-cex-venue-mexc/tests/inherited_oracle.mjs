// Live table/consumer witnesses while the shared TableHit representation lands.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const tables=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const dir='packages/safe-cex/src/exchanges/mexc/';
const census={};
const expressions=[];
for(const file of ['utils','api','exchange','ws-public','ws-private']) {
 const source=fs.readFileSync(path.join(root,dir+`mexc.${file}.ts`),'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const sites=[];
 const walk=node=>{
  if(ts.isElementAccessExpression(node) && /^(?:INTERVAL|BASE_URL|ORDER_SIDE|ORDER_STATUS|ORDER_TYPE|ORDER_TIME_IN_FORCE|POSITION_SIDE|POSITION_MODE|MARGIN_TYPE|REVERSE_.*|MEXC_ORDER_CANCEL_REASONS)$/.test(node.expression.getText(ast))) {
   sites.push(node.getText(ast));
   if(file==='exchange' && node.argumentExpression.getText(ast).startsWith('opts.')) expressions.push(node.getText(ast));
  }
  ts.forEachChild(node,walk);
 };
 walk(ast);census[file]=sites;
}
assert.deepEqual(Object.fromEntries(Object.entries(census).map(([k,v])=>[k,v.length])),{utils:10,api:1,exchange:15,'ws-public':1,'ws-private':0});
const names=['constructor','toString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','toLocaleString','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__'];
const witnesses=[];
for(const key of [...names,'unknown']) {
 for(const expression of [...new Set(expressions)]) {
  const target=expression.match(/opts\.([A-Za-z_$][\w$]*)\s*\]/)?.[1];
  assert(target,`unrecognized inherited lookup input: ${expression}`);
  const opts={interval:'missing-interval',type:'missing-type',timeInForce:'missing-time-in-force',side:'missing-side'};
  opts[target]=key;
  const hit=vm.runInNewContext(expression,{...tables,opts});
  const inherited=key!=='unknown';
  assert.equal(Boolean(hit),inherited,`${expression}: ${key}`);
  assert.equal(typeof hit,inherited?(key==='__proto__'?'object':'function'):'undefined');
  witnesses.push({expression,key,kind:inherited?(key==='__proto__'?'prototype':'function'):'missing',truthy:Boolean(hit),orFallback:!hit,nullishFallback:hit==null,negatedGuard:!hit});
 }
}
process.stdout.write(JSON.stringify({census,witnesses}));
