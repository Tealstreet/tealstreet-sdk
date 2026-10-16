import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import orderBy from 'lodash/orderBy.js';
const root=new URL('../../../packages/safe-cex/src/exchanges/bybit/',import.meta.url);
const base={orderId:'order',orderStatus:'Filled',cumExecQty:'2',avgPrice:'12',side:'Buy',updatedTime:'1700000000123',symbol:'BTCUSDT',cumExecFee:'-0.1',feeCurrency:'USDT'};
const values=[undefined,null,false,true,0,-0,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const encode=v=>typeof v==='number'?Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const cases=[];
for(const variant of ['bybit','bybitinverse']) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(file)==='mapOrderHistoryRowsToExecutions'));
 const method=cls.members.find(m=>m.name?.getText(file)==='mapOrderHistoryRowsToExecutions');
 const context=vm.createContext({orderBy,OrderSide:{Buy:'buy',Sell:'sell'},omitUndefined:v=>Object.fromEntries(Object.entries(v).filter(([,v])=>v!==undefined))});
 vm.runInContext(ts.transpileModule(`globalThis.map=${method.initializer.getText(file)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const add=(rows,opts={})=>{let result,error;try{result=encode(context.map(rows,'fallback',opts.orderId,opts.from,opts.to));}catch(e){error=e.message;}cases.push({variant,rows,opts,result,error});};
 for(const key of ['orderId','orderStatus','cumExecQty','avgPrice','side','updatedTime','cumExecFee'])for(const value of values){const row={...base};if(value===undefined)delete row[key];else row[key]=value;add([row]);}
 for(const status of ['Filled','PartiallyFilled','unfilled','FILLED','Cancelled',' Filled '])add([{...base,orderStatus:status}]);
 for(const side of ['Buy','Sell','buy','SELL',' Buy ','\uFEFFBuy'])add([{...base,side}]);
 for(const detail of [null,{},[],{'2':'1e16','1':'1','a':'1'},['-1','2','bad'],{USDT:'-1.25',USDC:'0.5'},'{"USDT":"1","bad":"\\ud800"}','{broken',{'USDT':{toString:null}}])add([{...base,cumExecFee:'bad',cumFeeDetail:detail}]);
 for(const ts of ['100000000000','100000000001','0x10','1e3','1700000000123'])for(const opts of [{},{from:1700000000124},{to:1700000000122},{from:1700000000123,to:1700000000123}])add([{...base,updatedTime:ts}],opts);
 for(const orderId of ['', 'order','other']) add([base,{...base,orderId:''}],{orderId});
 add([base,{...base,cumExecFee:'-9'}, {...base,orderId:'other',updatedTime:'1700000000124'},{...base,orderId:'older',updatedTime:'1699999999123'}]);
 add([{...base,avgPrice:'bad',cumExecValue:'30'}]);
 add([{...base,avgPrice:'12',cumExecValue:{toString:null}}]);
 add([{...base,cumExecFee:'0',cumFeeDetail:{USDT:{toString:null}}}]);
 add([null]);
 for(const key of ['symbol','feeCurrency'])for(const value of [12,true,{},['USDT']])add([{...base,[key]:value}]);
}
console.log(JSON.stringify(cases));
