import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function load(file,deps) {
 const source=fs.readFileSync(new URL(file,root),'utf8'),exports={};
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:id=>{assert.ok(id in deps,id);return deps[id];}});
 return exports;
}
const utility=load('utils/updateOrderBook.ts',{});
const {applyBinanceDepthUpdate:apply}=load('exchanges/binance/binance.orderbook.ts',{'../../utils/updateOrderBook':utility});
const norm=v=>typeof v==='number'?{number:Object.is(v,-0)?'-0':String(v)}:Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,norm(x)])):v;
const base={u:12,U:11,pu:10,b:[['9','5'],['10','0'],['8','2']],a:[['12','0'],['11','3']]};
const values=[undefined,null,false,true,0,10,11,12,13,'','0x10','12abc','NaN','Infinity','-Infinity','-0','1e400',[],[12],{}, {toString:null},'12'];
const updates=[];
for(const key of ['u','U','pu']) for(const value of values) updates.push(JSON.stringify({...base,[key]:value}));
const levels=[undefined,null,false,true,0,12,'','12','😀1',{},[],[[]],[null],[3],[true],['12'],[[1]],[[1,0]],[[null,null]],[[{},{}]],[[{toString:null},1]],[[1,{toString:null}]],[[9,2],null],[[9,2],[8,{toString:null}]],{length:2,0:['9','2'],1:['8','3']},{length:1.5,0:['9','2'],1:['8','3']},{length:'bad',0:null},{length:1,0:null},{length:{toString:null}},{length:1},[[9,2],[9,3]],[[9,'-0']],[[9,'NaN']],[[9,'Infinity']],[[9,'12abc']]];
for(const key of ['b','a']) for(const value of levels) updates.push(JSON.stringify({...base,[key]:value}));
for(const key of ['u','U','pu']) for(const literal of ['1e400','-1e400','-0']) updates.push('{'+Object.entries(base).map(([k,v])=>JSON.stringify(k)+':'+(k===key?literal:JSON.stringify(v))).join(',')+'}');
// U is coerced before final-ID/local-state guards; pu and level fields remain lazy.
for(const u of [undefined,'NaN','Infinity',10]) updates.push(JSON.stringify({...base,u,U:{toString:null}}));
for(const u of [undefined,'NaN','Infinity',10]) updates.push(JSON.stringify({...base,u,pu:{toString:null},b:[null],a:[null]}));
updates.push(JSON.stringify({...base,U:13,pu:{toString:null},b:[null]}));
const states=[{}, {snapshotLastUpdateId:10},{snapshotLastUpdateId:10,lastAppliedUpdateId:10},{snapshotLastUpdateId:0,lastAppliedUpdateId:11},{snapshotLastUpdateId:'NaN'},{snapshotLastUpdateId:'Infinity'},{snapshotLastUpdateId:'-Infinity'},{snapshotLastUpdateId:10,lastAppliedUpdateId:'NaN'},{snapshotLastUpdateId:10,lastAppliedUpdateId:'Infinity'},{snapshotLastUpdateId:10,lastAppliedUpdateId:'-Infinity'}];
const initial={bids:[{price:10,amount:1,total:41},{price:9,amount:1,total:42}],asks:[{price:11,amount:1,total:43},{price:12,amount:1,total:44}]};
const cases=[];
for(const row of updates) for(const stateInput of states) for(const previous of [false,true]) {
 const book=structuredClone(initial),state=Object.fromEntries(Object.entries(stateInput).map(([k,v])=>[k,Number(v)]));
 let result;try{result={value:apply(book,JSON.parse(row),state,{requirePreviousFinalUpdateId:previous})};}catch(e){result={error:{name:e.name,message:e.message}};}
 cases.push({row,state:stateInput,previous,initial,expected:{result,book:norm(book),state:norm(state)}});
}
process.stdout.write(JSON.stringify(cases));
