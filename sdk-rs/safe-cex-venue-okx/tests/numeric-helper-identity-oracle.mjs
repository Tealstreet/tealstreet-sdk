// Supplemental native-result bits; darwin arm64 baseline, Node v24.20.0.
import {loadSource} from './source-loader.mjs';import assert from 'node:assert/strict';
const base='../../../packages/safe-cex/src/exchanges/okx/';
const income=loadSource(new URL(base+'okx.income.ts',import.meta.url),['toNumber']);
const history=loadSource(new URL(base+'okx.order-history.ts',import.meta.url),['toNumber','toOptionalNumber']);
const closed=loadSource(new URL(base+'okx.closed-pnl.ts',import.meta.url),['toNumber']);
const position=loadSource(new URL('../../../packages/safe-cex/src/journal/okx.position-adapter.ts',import.meta.url),['num']);
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const values=[null,false,true,0,1,-1,1e-7,1e21,'','-0','0','1.25suffix','-.5','1e+tail','1e-3tail','0x10','Infinity','-Infinity','NaN','\uFEFF42\uFEFF','\u008542\u0085',[],[null],[1],[1,2],[[2]],{},'9'.repeat(400)];
const cases=[];for(const [op,fn]of Object.entries({income:income.toNumber,history:history.toNumber,optional:history.toOptionalNumber,closed:closed.toNumber,position:position.num}))for(const input of values){const value=fn(input);cases.push({op,input,present:value!==undefined,bits:value===undefined?null:bits(value)});}
assert.equal(cases.find(c=>c.op==='income'&&c.input==='-0').bits,'8000000000000000','source helper preserves string negative zero');
assert.equal(cases.find(c=>c.op==='income'&&c.input==='1.25suffix').bits,'3ff4000000000000','source helper finite payload');
process.stdout.write(JSON.stringify(cases));
