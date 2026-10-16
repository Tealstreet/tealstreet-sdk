// Source schemas come from the live numeric oracle; interfaces erase at runtime.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const source=spawnSync(process.execPath,[new URL('./numeric-ingress.mjs',import.meta.url).pathname],{encoding:'utf8'});
assert.equal(source.status,0,source.stderr);
const seen=new Set(),cases=[];
for(const row of JSON.parse(source.stdout)){
 const id=`${row.variant}:${row.record}`;
 if(seen.has(id))continue;
 seen.add(id);
 const base=JSON.parse(row.raw);base[row.key]=1;
 for(const depth of [1,120,140]){
  const raw=JSON.stringify(base).slice(0,-1)+',"unknown":'+'['.repeat(depth)+'0'+']'.repeat(depth)+'}';
  const parsed=JSON.parse(raw);
  let extra=parsed.unknown;
  for(let i=0;i<depth;i++){assert.ok(Array.isArray(extra));extra=extra[0];}
  assert.equal(extra,0,'Node retains deep unknown field');
  assert.equal(typeof parsed[row.key],'number');
  const bits=Buffer.alloc(8);bits.writeDoubleBE(parsed[row.key]);
  assert.equal(bits.toString('hex'),'3ff0000000000000','known source field remains one');
  cases.push({variant:row.variant,record:row.record,key:row.key,depth,base,raw});
 }
}
assert.equal(cases.length,12,'four actual source records at three depths');
process.stdout.write(JSON.stringify(cases));
