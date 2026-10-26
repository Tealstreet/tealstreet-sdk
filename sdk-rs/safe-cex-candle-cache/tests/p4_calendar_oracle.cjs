const path=require('node:path');const {root,load}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const base=path.join(root,'packages/safe-cex/src');const calendar=load(path.join(base,'utils/calendarCandles.ts'));const types=load(path.join(base,'types.ts'));
const functions=Object.fromEntries(['getCandleStartTime','getCandleEndTime','getNumCandlesInRange','splitTimeRange'].map(k=>[k,load(path.join(base,'utils',k+'.ts'))[k]]));
const enc=x=>typeof x==='number'?(Number.isNaN(x)?'NaN':Object.is(x,-0)?'-0':String(x)):x;const rows=[];
const times=[-Infinity,NaN,Infinity,-8640000000000001,-8640000000000000,-62167219200000,-59011459200000,-1,-0,0,0.001,59.999,60,345600,604800,1000000000000,1000000000000.5,1709164800000,8640000000000000,8640000000000001];
for(const period of ['1W','1M'])for(const time of times)for(const op of ['calendarPeriodStartMs','nextCalendarPeriodStartMs'])rows.push({op,period,time:enc(time),expected:enc(calendar[op](period,time))});
for(const seconds of [...times,604800,2592000])rows.push({op:'calendarPeriodForChartSeconds',time:enc(seconds),expected:calendar.calendarPeriodForChartSeconds(seconds)||null});
for(const interval of [...Object.keys(types.timeframeToSeconds),'1W','W','M','bad',...Object.getOwnPropertyNames(Object.prototype)]) {
 for(const time of times)for(const op of ['getCandleStartTime','getCandleEndTime'])rows.push({op,interval,time:enc(time),expected:enc(functions[op](interval,time))});
 const live=calendar.createLiveCandleStart(interval);for(const time of times)rows.push({op:'createLiveCandleStart',interval,time:enc(time),expected:enc(live(time))});
 for(const [from,to] of [[0,0],[1,0],[0,61],[-100,100],[1000000000001,1000001000001],[NaN,60]]){let expected;try{expected={ok:enc(functions.getNumCandlesInRange(interval,from,to))}}catch(e){expected={error:e.message}}rows.push({op:'getNumCandlesInRange',interval,from:enc(from),to:enc(to),expected});}
}
for(const [from,to] of [[0,0],[0,10],[5,1],[-5,12],[Infinity,10],[0,NaN]])for(const span of [-1,0,0.5,1,3,10,NaN,Infinity])rows.push({op:'splitTimeRange',from:enc(from),to:enc(to),span:enc(span),expected:functions.splitTimeRange(from,to,span)});
console.warn=()=>{};process.stdout.write(JSON.stringify(rows));
