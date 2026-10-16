import assert from 'node:assert/strict';
import {loadSource} from './source_loader.mjs';
import fs from 'node:fs';
const {createIncrementalFills} = loadSource(new URL('../../../packages/safe-cex/src/utils/incrementalFills.ts', import.meta.url));
const cases = [];
for (const capacity of [0,-1,2,500,'NaN','Infinity']) {
  const fills = createIncrementalFills(Number(capacity));
  const frames = [ ['A',2,20], ['A',2,99], ['A',1,1], ['A','NaN',100], ['A',3], ['A',4,40], ['A',5,35], ['A',6,40], ['B',1], ['A',6], ['C',1], ['B',2], ['A',7], ['A',8,'NaN'], ['A',9,80], ['D',100000,1e9], ['D',100000.1,1000000012.3], ['D',100000.30000000001,1000000012.31], ['E',0.1,0.1], ['E',0.3,0.3], ['F',1e21], ['F',2e21,1e-7], ['F',2.5e21,3e-7] ];
  const steps = frames.map(([id,qty,value]) => ({id,qty,value:value ?? null,expected:fills(id,Number(qty),value === undefined ? undefined : Number(value))}));
  cases.push({capacity,steps});
}
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(cases.length, 6);
for (const c of cases) assert.equal(c.steps.length, 23);
const path = new URL('./fills_oracle.json',import.meta.url);
const output = JSON.stringify(cases,null,2)+'\n';
if (process.argv.includes('--check')) {
  if (fs.readFileSync(path,'utf8') !== output) throw new Error('fills oracle fixtures changed');
  console.log(`${cases.reduce((n,c)=>n+c.steps.length,0)} incremental-fill source-oracle frames verified`);
} else fs.writeFileSync(path,output);
