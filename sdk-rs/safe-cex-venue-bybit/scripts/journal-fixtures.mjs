import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../../../packages/safe-cex/src/journal/bybit.position-adapter.ts',import.meta.url),'utf8');
const file=ts.createSourceFile('adapter',source,ts.ScriptTarget.Latest,true);
const declarations=new Map(file.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).map(d=>[d.name.getText(file),d.initializer]));
const context=vm.createContext({PositionSide:{Long:'long',Short:'short'}});
for(const name of ['num','CLOSE_REASON','foldRun'])vm.runInContext(ts.transpileModule(`globalThis.${name}=${declarations.get(name).getText(file)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const originalFold=context.foldRun;
vm.runInContext(ts.transpileModule(`globalThis.aggregate=${declarations.get('aggregateBybitClosedPnl').getText(file)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const base={symbol:'DOGEUSDT',orderId:'one',side:'Sell',avgEntryPrice:'0.07088',closedSize:'1',openFee:'0.01',closeFee:'0.02',closedPnl:'0.2',avgExitPrice:'0.08',cumExitValue:'0.08',updatedTime:'1700000000123',createdTime:'1700000000000',leverage:'10',execType:'Trade'};
const values=[undefined,null,false,true,0,-0,-1,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const cases=[];
const encode=v=>typeof v==='number'?Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const add=(rows,legs={},funding=0)=>{
 const runs=[];let error;const legCalls=[];
 const deps={account:'acct',resolveSettleCurrency:()=> 'USDT',resolveLeg:id=>{legCalls.push(id);return Object.hasOwn(legs,id)?legs[id]:undefined;},resolveContext:()=>({funding,fundingAttributed:true})};
 context.foldRun=(run,deps)=>{const result=originalFold(run,deps);runs.push({rows:run,result});return result;};
 try{context.aggregate(rows,deps);}catch(e){error=e.message;}
 cases.push({rows,legs,funding,runs:runs.map(({rows,result})=>({rows,result:encode(result)})),error});
};
for(const key of ['avgEntryPrice','closedSize','openFee','closeFee','closedPnl','avgExitPrice','cumExitValue','updatedTime','createdTime'])for(const value of values){const row={...base};if(value===undefined)delete row[key];else row[key]=value;add([row]);add([base,{...row,orderId:'two'}]);}
add([base,{...base,symbol:'BTCUSDT',orderId:'btc',updatedTime:'1700000000124'},{...base,orderId:'two',avgEntryPrice:'0.070880',updatedTime:'1700000000125'}]);
add([base,{...base,orderId:'two',updatedTime:'1700009999999'}]); // Same-entry reopen stays merged: source bug.
add([base,{...base,orderId:'two'}],{one:'long',two:'short'});
add([base,{...base,orderId:'two',avgEntryPrice:'0.08'}]);
for(const funding of [-0.3,0,0.7]) add([base,{...base,orderId:'two'}],{},funding);
add([]);add([null]);
console.log(JSON.stringify(cases));
