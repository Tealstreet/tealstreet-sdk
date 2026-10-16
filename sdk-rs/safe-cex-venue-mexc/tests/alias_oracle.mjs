import {load} from './source.mjs';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
if(process.argv.includes('--prototype')) {
 const raw=[{currency:'__proto__',free:7,total:9},{currency:'constructor',free:2}];
 const currencies=api.normalizeBalance(raw).currencies;
 process.stdout.write(JSON.stringify({
   raw,ownKeys:Object.keys(currencies),hasOwnFree:Object.hasOwn(currencies,'free'),
   inheritedFree:currencies.free,prototypeTotal:Object.getPrototypeOf(currencies).total,
   constructorFree:currencies.constructor.free,
 }));
 process.exit(0);
}
const base={symbol:'BTC_USDT',id:'42',orderId:'43',side:1,positionType:2,vol:5,holdVol:7,price:123,contractSize:0.5,currency:'USDT',probe:1,nested:{probe:1}};
const cases=[];
for(const name of ['normalizeMarket','normalizeTicker','normalizePosition','normalizeOrder','normalizePlanOrder','normalizeStopOrder','normalizeBalance']){
 const raw=structuredClone(base);
 const output=name==='normalizeBalance'?api[name]([raw]):api[name](raw,api.normalizeMarket(raw));
 const info=name==='normalizeBalance'?output.info[0]:output.info;
 const view=()=>({input:raw.probe,output:info.probe,inputNested:raw.nested.probe,outputNested:info.nested.probe,...(name==='normalizeBalance'?{currency:output.currencies.USDT.info.probe,currencyNested:output.currencies.USDT.info.nested.probe}:{})});
 raw.probe=2;raw.nested.probe=2;
 const afterInput=view();
 info.probe=3;info.nested.probe=3;
 cases.push({name,raw:base,afterInput,afterOutput:view()});
}
process.stdout.write(JSON.stringify(cases));
