import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../../../packages/safe-cex/src/exchanges/bybit/',import.meta.url);
const base={execId:'fill1',orderId:'order1',symbol:'BTCUSD',execType:'Trade',execQty:'2',execPrice:'3',execTime:'1700000000123',side:'Buy',execFee:'.1',feeCurrency:'USD',isMaker:true};
const values=[undefined,null,false,true,0,-0,-1,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const mark=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':'finite';
const cases=[];
for(const variant of ['bybit','bybitinverse']){
 const file=ts.createSourceFile(variant,readFileSync(new URL(`${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(file)==='fetchHistoricalFills'));
 const member=cls.members.find(m=>m.name?.getText(file)==='fetchHistoricalFills');let loop;
 const walk=n=>{if(ts.isForOfStatement(n)&&n.expression.getText(file)==='executionsList')loop=n;ts.forEachChild(n,walk);};walk(member);assert.ok(loop);
 const context=vm.createContext({OrderSide:{Buy:'buy',Sell:'sell'}});
 vm.runInContext(ts.transpileModule(`globalThis.map=(executionsList,symbol,executions)=>{${loop.getText(file)}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const add=rows=>{let error;const result=[];try{context.map(rows,'fallback',result);}catch(e){error=e.message;}cases.push({variant,rows,result,error,classes:result.map(r=>[r.timestamp,r.price,r.qty,r.commission].map(mark))});};
 for(const key of ['execType','execQty','execPrice','side','execTime','execFee','isMaker'])for(const value of values){const row={...base};if(value===undefined)delete row[key];else row[key]=value;add([row]);add([base,row]);}
 for(const execType of ['Funding',' settle ','SETTLEMENT','Delivery','SessionSettlePnl','UnknownFutureTrade','\uFEFFFunding'])add([{...base,execType}]);
 for(const side of ['buy',' sell ','\uFEFFBuy','\u0085Buy'])add([{...base,side}]);
 for(const execTime of [100000000000,100000000001,1700000000.5,'0x10','1e400'])add([{...base,execTime}]);
 for(const execValue of ['6','0','Infinity',{toString:null}])add([{...base,execPrice:'0',execValue}]);
 add([{...base,execQty:null,closedSize:'3',execPrice:null,avgPrice:'2',execTime:null,tradeTime:'12',execFee:null,fee:'.2',side:null,orderSide:'Sell'}]);
 add([{...base,execQty:'0',closedSize:'3',execPrice:'0',closedPnl:'6'}]);
 add([base,null]);add([null]);add([]);
}
console.log(JSON.stringify(cases));
