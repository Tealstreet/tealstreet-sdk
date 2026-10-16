import assert from 'node:assert/strict';
import fs from 'node:fs';
import {sourceStore,requestRecorder} from './store_source.mjs';
const context=sourceStore(), store=new context.Store(),record=requestRecorder();
let notifications=0;store.subscribe(()=>notifications++);
const steps=[];
for(const hadResponse of [false,true]) for(const status of [undefined,199,200,299,300,401,403,418,429,500]) for(const capture of [false,true]) {
  record({store},status,hadResponse,{capture});
  steps.push({status:status??null,hadResponse,capture,counts:store.serialize().requestStats,notifications});
}
const beforeReset=store.serialize().requestStats;store.reset();
const data={steps,beforeReset,afterReset:store.serialize().requestStats,notifications};
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(steps.length, 40);
const path=new URL('./request_oracle.json',import.meta.url),output=JSON.stringify(data,null,2)+'\n';
if(process.argv.includes('--check')){if(fs.readFileSync(path,'utf8')!==output)throw Error('request oracle drift');}else fs.writeFileSync(path,output);
console.log(`${steps.length} request source-oracle frames verified`);
