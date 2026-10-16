import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire} from 'node:module';import path from 'node:path';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts',import.meta.url);
let source=fs.readFileSync(url,'utf8');
if(process.argv.includes('--mutate-batch-size'))source=source.replaceAll('const batchSize = 100;','const batchSize = 99;');
const ast=ts.createSourceFile(url.pathname,source,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXPublicWebsocket');
const member=owner.members.find(n=>n.name?.getText(ast)==='subscribe');assert.ok(member);
const require=process.env.SAFE_CEX_TYPESCRIPT_PATH?createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH),'oracle.cjs')):createRequire(import.meta.url);
const flatten=require(process.argv.includes('--mutate-missing-package')?'missing-okx-flatten-control':'lodash/flatten');
const module={exports:{}};const code=ts.transpileModule('export class Selected {'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(module,exports,flatten){'+code+'\n})')(module,module.exports,flatten);
if(process.argv.includes('--alias-witness')){
 const receiver=new module.exports.Selected();receiver.topics={tickers:Array.from({length:101},(_,i)=>({channel:'tickers',instId:'BTC-'+i}))};
 const before=JSON.parse(JSON.stringify(receiver.topics));const sent=[];
 receiver.send=text=>{sent.push(text);if(sent.length===1)receiver.topics.tickers[100].instId='MUTATED-AFTER-FIRST-SEND';};receiver.subscribe();
 assert.equal(before.tickers[100].instId,'BTC-100');assert.equal(JSON.parse(sent[1]).args[0].instId,'MUTATED-AFTER-FIRST-SEND','source live topic alias after first send');
 process.stdout.write(JSON.stringify({before,after:receiver.topics,sent}));process.exit(0);
}
if(process.argv.includes('--surrogate-witness')){
 const receiver=new module.exports.Selected();receiver.topics=JSON.parse('{"topic":[{"instId":"\\ud800"}]}');let wire;
 receiver.send=text=>wire=text;receiver.subscribe();
 assert.equal(typeof receiver.topics.topic[0].instId,'string');
 assert.equal(receiver.topics.topic[0].instId.charCodeAt(0),0xd800);
 process.stdout.write(JSON.stringify({wire}));process.exit(0);
}
const cases=[];
function run(topics,failSendAt){const receiver=new module.exports.Selected();receiver.topics=topics;const sent=[];receiver.send=text=>{const index=sent.length;sent.push(text);if(index===failSendAt)throw Error('send failure');};let error;try{receiver.subscribe();}catch(e){error={name:e.name,message:e.message};}cases.push({topics,failSendAt,sent,error});return cases.at(-1);}
for(const size of [0,1,99,100,101,199,200,201,1501])for(const failSendAt of [undefined,0,1]){
 const topics={tickers:Array.from({length:size},(_,i)=>({channel:'tickers',instId:'BTC-'+i}))};const c=run(topics,failSendAt);
 const batches=Math.ceil(size/100);assert.equal(c.sent.length,failSendAt===undefined?batches:Math.min(batches,failSendAt+1),'subscription batch count');
 if(c.sent.length)assert.equal(JSON.parse(c.sent[0]).args.length,Math.min(size,100),'subscription batch width');
}
run({'z':[{id:'z'}],'10':[{id:'ten'}],'2':[{id:'two'}],'01':[{id:'leading'}],'a':[{id:'a'}]});
assert.deepEqual(JSON.parse(cases.at(-1).sent[0]).args.map(x=>x.id),['two','ten','z','leading','a'],'subscription own-key order');
for(const value of [null,false,0,'scalar',{},[[{id:'nested'}]],[{channel:'\uFEFF',instId:'astral 😀'}]])run({topic:value});
assert.equal(cases.length,35);
process.stdout.write(JSON.stringify(cases));
