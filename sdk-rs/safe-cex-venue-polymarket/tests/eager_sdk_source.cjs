// Whole maintained class and Store; only terminal xhr is held.
const assert=require('node:assert/strict');
const {PolymarketExchange}=require('../../../packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts');
const {DefaultStore}=require('../../../packages/safe-cex/src/store/store.base.ts');
(async()=>{
 const venue=new PolymarketExchange('polymarket',{key:'',secret:'',publicDataOnly:true,tuning:{consoleLogging:false}},new DefaultStore());
 const warnings=[];venue.emitter.on('log',(...a)=>warnings.push(a));
 const unit=venue.setLeverage('event-Yes',15);assert.equal(warnings.length,1);
 const gamma={conditionId:'condition',slug:'event',question:'Question?',description:'description',endDate:'2026-12-01',active:true,closed:false,enableOrderBook:true,acceptingOrders:true,clobTokenIds:'["123","456"]',outcomes:'["Yes","No"]',orderPriceMinTickSize:'0.01',orderMinSize:'5',negRisk:false};
 let release;const held=new Promise(r=>{release=r;});const calls=[];
 venue.api.gammaXhr.get=function(...args){assert.strictEqual(this,venue.api.gammaXhr);calls.push(args);return held;};
 const waiter=venue.fetchMarkets();assert.equal(calls.length,1,'source typed prefix');
 release({data:[gamma]});const markets=await waiter;assert.equal(markets.length,2);
 assert.strictEqual(venue.marketMetadata.get('condition'),gamma);assert.equal(venue.tokenToSymbol.get('123'),'event-Yes');
 await unit;venue.dispose();
 process.stdout.write(JSON.stringify({warnings,calls,gamma,marketCount:markets.length,symbol:venue.tokenToSymbol.get('123')}));
})().catch(e=>{console.error(e);process.exitCode=1;});
