import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const encode=v=>typeof v==='number'?Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const helpers={exports:{}};vm.createContext(helpers);
for(const name of ['updateOrderBook','orderbook']) {
 const source=readFileSync(new URL(`utils/${name}.ts`,root),'utf8');
 vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,helpers);
}
const cases=[];
const snapshot={type:'snapshot',data:{u:1,a:[['12','3'],['14','2']],b:[['10','1'],['8','2']]}};
const values=[null,false,true,0,-0,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
for(const variant of ['bybit','bybitinverse','bybitspot']) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`exchanges/bybit/${variant}.ws-public.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(ts.isClassDeclaration);
 const member=cls.members.find(m=>m.name?.getText(file)==='listenOrderBook');
 const statements=[];
 for(const statement of member.initializer.body.statements) {
  if(statement.getText(file).startsWith('const waitForConnectedAndSubscribe'))break;
  statements.push(statement.getText(file));
 }
 assert.ok(statements.some(s=>s.startsWith('const parser')));
 const context=vm.createContext({...helpers.exports});
 const make=vm.runInContext(ts.transpileModule(`(function(callback,setTimeout){const symbol='BTCUSDT';${statements.join('\n')}return {parser,state:()=>({book:orderBook,lastUpdate:lastUpdateId,resyncing:isResyncing})};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText.replace(/;\s*$/,''),context);
 const add=frames=>{
  let emitted;const timers=[];let sends=0;
  const {parser,state}=make.call({isDisposed:false,send(){sends++;}},book=>emitted=encode(book),(f)=>{timers.push(f);return timers.length;});
  const outcomes=frames.map(frame=>{emitted=undefined;const before=sends;let error;
   try{if(frame==='timer'){timers.splice(0).forEach(f=>f());}else parser(frame);}catch(e){error=e.message;}
   return {state:encode(state()),emitted,error,newResync:frame!=='timer'&&sends>before};
  });cases.push({variant,frames,outcomes});
 };
 for(const key of ['a','b']) for(const column of [0,1]) for(const value of values) {
  const row=['11','4'];row[column]=value;
  add([snapshot,{type:'delta',data:{u:2,[key]:[row]}}]);
  add([{type:'snapshot',data:{u:1,[key]:[row]}}]);
  add([snapshot,{type:'delta',data:{u:2,[key]:[['11','5'],row]}}]);
 }
 for(const row of ['12','','1x','💥','💥1',{'0':'11','1':'4'},{'0':{toString:null},'1':'4'},false,12]) {
  add([snapshot,{type:'delta',data:{u:2,a:[row]}}]);
  add([{type:'snapshot',data:{u:1,a:[row]}}]);
 }
 for(const levels of [{'0':['11','4'],length:'1'}, '12', {length:1}]) add([{type:'snapshot',data:{u:1,a:levels}}]);
 for(const first of [0,1,9,'9','10',true,[],{}, {toString:null}]) for(const next of [0,1,2,10,11,12,'10','11','91',null]) add([{...snapshot,data:{...snapshot.data,u:first}},{type:'delta',data:{u:next,a:[['12','0']]}}]);
 add([snapshot,{type:'delta',data:{u:4}}, {type:'snapshot',data:{u:10,a:[['12','1']]}},{type:'delta',data:{u:13}},'timer',{type:'delta',data:{u:14}}]);
 add([snapshot,{type:'snapshot',data:{u:2,a:[['12','9'],null]}}]);
 add([snapshot,{type:'delta',data:{u:2,a:[['12','9'],null]}}]);
 add([snapshot,{type:'other',data:{}},null]);
 add([{type:'snapshot',data:{u:1,a:[['14','2'],['12','3']],b:[['8','1'],['10','2']]}},{type:'delta',data:{u:2,a:[['13','4']]}}]);
}
console.log(JSON.stringify(cases));
