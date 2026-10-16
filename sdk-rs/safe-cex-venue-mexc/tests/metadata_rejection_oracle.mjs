import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root, ts, load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='MexcExchange');
const methods=['fetchLeverageForSymbol','updateSymbolMetadata'].map(name=>{
 const member=owner.members.find(n=>n.name?.getText(ast)===name);assert(member,`actual source method ${name}`);return member.getText(ast);
});
const helpers=source.slice(source.indexOf('const extractData ='),source.indexOf('\n\ntype MexcSymbolConfig'));
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const context={toNumber:utils.toNumber,normalizeMarginType:utils.normalizeMarginType,ENDPOINTS:types.ENDPOINTS,REVERSE_MARGIN_TYPE:types.REVERSE_MARGIN_TYPE,LogSeverity:{Warning:'warning'}};
vm.runInNewContext(ts.transpileModule(helpers+'\nclass Holder{'+methods.join('\n')+'}\nglobalThis.Holder=Holder;',
 {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const output=[];
for(const publicOnly of [false,true]) {
 const store=new DefaultStore();
 store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:1,maxLeverage:20})]},false);
 const errors=[Object.assign(new Error('metadata first'),{code:401}),Object.assign(new Error('metadata second'),{code:402})];
 let first=0,later=0,requests=0;
 const logs=[];
 store.subscribe(()=>{throw errors[first++];});
 store.subscribe(()=>{later++;});
 const holder=new context.Holder();
 Object.assign(holder,{store,isPublicDataOnly:publicOnly,symbolSettings:new Map([['BTCUSDT',{marginType:'cross',buyLeverage:21,sellLeverage:7,positionMode:'hedged'}]]),log(message,severity){logs.push([message,severity]);},xhr:{get:async()=>{requests++;return {data:[{positionType:1,leverage:'31',maxLeverageView:'80',openType:1},{positionType:2,leverage:'12',maxLeverage:'60',openType:2}]};}}});
 const pending=holder.updateSymbolMetadata('BTCUSDT');
 const prefix={first,later,requests};
 assert.equal(first,publicOnly?1:0,'SOURCE metadata eager subscriber prefix');
 assert.equal(requests,publicOnly?0:1,'SOURCE metadata eager request prefix');
 let error;try{await pending;}catch(e){error=e;}
 const expected=publicOnly?errors[0]:errors[1];
 assert.equal(error,expected,'SOURCE metadata original final error identity');
 assert.equal(first,publicOnly?1:2,'SOURCE metadata independent subscriber calls');
 assert.equal(later,0,'SOURCE metadata aborts later subscriber');
 assert.equal(requests,publicOnly?0:1,'SOURCE metadata independent final request calls');
 assert.deepEqual(logs,publicOnly?[]:[['Failed to fetch MEXC leverage cap for BTCUSDT: metadata first','warning']],'SOURCE metadata independent warning-log calls and fields');
 assert.equal(store.marketsBySymbol.BTCUSDT.limits.leverage.max,publicOnly?20:60,'SOURCE metadata committed market cap');
 assert.equal(store.fetchedLeverage.BTCUSDT.buyLeverage,21,'SOURCE metadata committed fallback buy');
 assert.equal(store.fetchedLeverage.BTCUSDT.sellLeverage,7,'SOURCE metadata committed fallback sell');
 output.push({publicOnly,prefix,first,later,requests,logs,code:error.code,message:error.message,max:store.marketsBySymbol.BTCUSDT.limits.leverage.max,buy:store.fetchedLeverage.BTCUSDT.buyLeverage,sell:store.fetchedLeverage.BTCUSDT.sellLeverage});
}
assert.equal(output.length,2,'SOURCE metadata two complete wrapper/child roots');
console.log(JSON.stringify(output));
