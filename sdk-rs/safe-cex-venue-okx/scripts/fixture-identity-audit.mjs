import fs from 'node:fs';import path from 'node:path';import {spawnSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const directory=path.join(root,'sdk-rs/safe-cex-venue-okx/tests');
const sources=fs.readdirSync(directory).filter(x=>x.endsWith('-oracle.mjs')&&x!=='field-identity-oracle.mjs'&&x!=='numeric-helper-identity-oracle.mjs').sort();
const report={method:'Execute original oracle generators, recursively inspect every emitted object, group sibling scalar fields by JS runtime type and exact value; count equal pairs as candidates, not defects. Schemas with distinct-value witnesses are reported separately. This includes input, expected output, trace and metadata objects.',files:[],records:0,candidates:0,nullPairs:0};
for(const name of sources){
 const run=spawnSync(process.execPath,[path.join(directory,name)],{encoding:'utf8',maxBuffer:128*1024*1024});
 if(run.status!==0)throw Error(`${name} exited ${run.status}: ${run.stderr}`);
 const value=JSON.parse(run.stdout);const pairs=new Map();let records=0,candidates=0,nullPairs=0;
 function walk(value,where){
  if(Array.isArray(value)){value.forEach((v,i)=>walk(v,`${where}[${i}]`));return;}
  if(value===null||typeof value!=='object')return;
  records++;const nullFields=Object.values(value).filter(v=>v===null).length;nullPairs+=nullFields*(nullFields-1)/2;const entries=Object.entries(value).filter(([,v])=>v!==null&&['string','number','boolean'].includes(typeof v));
  for(let i=0;i<entries.length;i++)for(let j=i+1;j<entries.length;j++){
   const [a,x]=entries[i],[b,y]=entries[j];if(typeof x!==typeof y)continue;
   const key=[a,b].sort().join('/');const entry=pairs.get(key)??{fields:key,type:typeof x,equal:0,distinct:0,example:where};
   if(Object.is(x,y)){entry.equal++;candidates++;}else entry.distinct++;
   pairs.set(key,entry);
  }
  for(const [key,v] of Object.entries(value))walk(v,where+'.'+key);
 }
 walk(value,'$');report.files.push({file:name,records,candidates,nullPairs,pairs:[...pairs.values()].filter(p=>p.equal>0).sort((a,b)=>a.fields.localeCompare(b.fields))});report.records+=records;report.candidates+=candidates;report.nullPairs+=nullPairs;
}
process.stdout.write(JSON.stringify(report,null,2)+'\n');
