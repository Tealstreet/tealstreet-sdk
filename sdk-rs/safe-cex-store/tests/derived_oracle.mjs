// Live source oracle; generated outputs stay in memory, never committed.
import fs from 'node:fs';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
import vm from 'node:vm';
const root=new URL('../../../',import.meta.url);
function body(path) {
 const source=fs.readFileSync(new URL(path,root),'utf8');const ast=ts.createSourceFile(path,source,ts.ScriptTarget.Latest,true);
 return ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
}
const context=vm.createContext({OrderStatus:{Open:'open'},OrderType:{Limit:'limit',TrailingStopLoss:'trailing_stop_market'},OrderSide:{Buy:'buy',Sell:'sell'},PositionSide:{Long:'long',Short:'short'},PositionMode:{Hedged:'hedged'}});
const source=body('packages/safe-cex/src/utils/safe-math.ts')+'\n'+body('packages/safe-cex/src/utils/risk.ts')+'\n'+body('packages/safe-cex/src/utils/roeRoi/calculateReturns.ts')+'\n'+body('packages/safe-cex/src/utils/roeRoi/calculateRelativeReturns.ts');
vm.runInContext(ts.transpileModule(source+'\nglobalThis.api={calculateOrdersRisk,calculateRiskAtPrice,formatPnl,calculateReturns,calculateRelativeReturns};',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const p=(side='long',contracts=2,entryPrice=100,mode)=>({id:'p',symbol:'BTC',side,entryPrice,contracts,positionMode:mode,notional:200,leverage:5,unrealizedPnl:10,liquidationPrice:0});
const o=(id,price,remaining,type='limit',side='sell',reduceOnly=true)=>({id,symbol:'BTC',status:'open',type,side,price,amount:remaining,filled:0,remaining,reduceOnly,lastUpdate:0});
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},makerFee:0.001,takerFee:0.002};
const ticker={id:'BTC',symbol:'BTC',bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const scenarios=[
 {positions:[p()],orders:[o('tp',110,1),o('stop',90,2,'stop_market')]},
 {positions:[p()],orders:[o('flip',110,3,'limit','sell',false),o('next',120,1,'limit','buy',false),o('next-stop',125,1,'stop_market','buy')]},
 {positions:[p()],orders:[o('full',110,0,'take_profit_market'),o('later',120,1,'limit','sell',false)]},
 {positions:[p()],orders:[o('add',90,1,'limit','buy',false),o('stop',80,0,'stop_market')]},
 {positions:[p()],orders:[o('trigger',90,3,'stop_market','sell',false),o('late',80,1,'stop_market')]},
 {positions:[p()],orders:[o('trail',90,1,'trailing_stop_market')]},
 {positions:[p('long',0)],orders:[o('entry',90,1,'limit','buy',false),o('short',110,1,'limit','sell',false)]},
 {positions:[p('long',2,100,'hedged'),p('short',0,100,'hedged')],orders:[o('hedge',110,1,'limit','sell',false),o('reduce',120,1)]},
 {positions:[p('long',2,100,'hedged'),p('short',2,100,'hedged')],orders:[o('stop',90,1,'stop_market'),o('tp',110,1)]},
 {positions:[p('short')],orders:[o('tp',90,1,'limit','buy'),o('add',110,1,'limit','sell',false),o('stop',120,0,'stop_market','buy')]},
 {positions:[p()],orders:[{...o('closed',110,1),status:'closed'},o('same',100,1),o('same',100,0.5)]},
 {positions:[],orders:[o('buy',95,1,'limit','buy',false),o('sell',105,1,'limit','sell',false)]},
];
scenarios.push({positions:[],orders:Array.from({length:64},(_,i)=>o('nan-'+i,i%11===0?NaN:(i*37)%67,1,'limit','sell',false))});
const risks=[];
for(const scenario of scenarios) for(const last of [0,95,100,105]) for(const withMarket of [false,true]) {
 const input={...scenario,tickers:[{...ticker,last}],markets:withMarket?[market]:[]};
 const risk=context.api.calculateOrdersRisk(input.markets,input.tickers,input.positions,input.orders);
 const prices=[80,90,95,100,105,110,120,130];
 risks.push({input,risk,crosshairs:prices.map(price=>({price,expected:context.api.calculateRiskAtPrice(price,'BTC',risk,input.positions,input.orders)}))});
}
const prototypeFailures=[];
for(const key of ['__proto__','constructor','toString'])for(const field of ['symbol','id']) {
 const input={markets:[market],tickers:[ticker],positions:[p()],orders:[{...o('normal',110,1),[field]:key}]};
 let failed=false;try{context.api.calculateOrdersRisk(input.markets,input.tickers,input.positions,input.orders);}catch{failed=true;}
 prototypeFailures.push({input,failed});
}
const returns=[];
for(const side of ['long','short']) for(const entry of [0,100,-100]) for(const contracts of [0,2,-2]) for(const override of [undefined,0,-0,10,-20]) {
 const position=p(side,contracts,entry);returns.push({position,override:override??null,exitPrice:110,expected:context.api.calculateReturns(position,override),relative:context.api.calculateRelativeReturns(position,110)});
}
const specialReturns=[];
for(const patch of [{override:NaN},{override:Infinity},{override:-Infinity},{upnl:NaN},{entry:NaN},{entry:Infinity},{contracts:Infinity},{leverage:Infinity},{exit:NaN},{exit:Infinity}]) {
 const args={entry:100,contracts:2,upnl:10,leverage:5,exit:110,override:undefined,...patch};
 const position={...p(),entryPrice:args.entry,contracts:args.contracts,unrealizedPnl:args.upnl,leverage:args.leverage};
 specialReturns.push({args,expected:context.api.calculateReturns(position,args.override),relative:context.api.calculateRelativeReturns(position,args.exit)});
}
const formats=[];
for(const value of [0,-0,0.00001,-0.00001,2.675,-2.675,999.995,999999.999,1e21,1e-7,1234.5678,NaN,Infinity,-Infinity])for(const precision of [0,2,4])for(const prefix of [undefined,'','$']) formats.push({value,precision,prefix:prefix??null,expected:context.api.formatPnl(value,precision,prefix)});
process.stdout.write(JSON.stringify({risks,returns,specialReturns,formats,prototypeFailures},(_,v)=>typeof v==="number"&&(Object.is(v,-0)||!Number.isFinite(v))?{$number:Object.is(v,-0)?"-0":String(v)}:v));
