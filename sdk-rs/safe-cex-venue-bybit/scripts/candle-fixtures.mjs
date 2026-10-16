import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../../../packages/safe-cex/src/exchanges/bybit/',import.meta.url);
const base={start:1000,open:'10',high:'12',low:'8',close:'11',volume:'3'};
const values=[null,false,true,0,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const encode=v=>typeof v==='number'?Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v:v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const cases=[];
for(const variant of ['bybit','bybitinverse','bybitspot']) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`${variant}.ws-public.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(ts.isClassDeclaration);
 const member=cls.members.find(m=>m.name?.getText(file)==='listenOHLCV');
 let parser;
 const walk=node=>{if(ts.isVariableDeclaration(node)&&node.name.getText(file)==='parser')parser=node.initializer;ts.forEachChild(node,walk);};walk(member);assert.ok(parser);
 const make=vm.runInNewContext(ts.transpileModule(`(function(callback){let lastCandle;const topic='topic',secondaryTopic='trade';const parser=${parser.getText(file)};return {parser,state:()=>lastCandle};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText.replace(/;\s*$/,''));
 const add=rows=>{
  let emitted;
  const {parser,state}=make.call({armOhlcvNoDataTimeout(){}},value=>emitted=structuredClone(value));
  const outcomes=rows.map(raw=>{emitted=undefined;let error;try{parser({data:[raw]});}catch(e){error=e.message;}return {emitted:encode(emitted),state:encode(state()),error};});
  cases.push({variant,rows,outcomes});
 };
 for(const field of ['start','open','high','low','close','volume']) for(const value of values) {
  add([{...base,[field]:value}]);
  add([base,{...base,[field]:value}]);
  add([base,{...base,start:2000,[field]:value}]);
  add([base,{...base,start:0,[field]:value}]);
 }
 add([base,{...base,start:2000,high:'NaN'}, {...base,start:2000,high:'20'}]);
 add([base,{...base,open:{toString:null}}]); // Same candle never reparses open.
 add([base,null]);
 add([base,{...base,start:'NaN',high:'NaN'},base]);
}
console.log(JSON.stringify(cases));
