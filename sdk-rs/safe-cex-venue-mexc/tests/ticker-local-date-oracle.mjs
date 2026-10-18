import {load} from './source.mjs';
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const market=utils.normalizeMarket({symbol:'BTC_USDT',baseCoin:'BTC',quoteCoin:'USDT',contractSize:1,maxLeverage:125});
const cases=[
 ['local','2026-10-17T08:00:00'],
 ['utc','2026-10-17T08:00:00Z'],
 ['date','2026-10-17'],
 ['numeric',1792224000000],
];
process.stdout.write(JSON.stringify(cases.map(([id,nextSettleTime])=>({id,fundingTime:utils.normalizeTicker({symbol:'BTC_USDT',lastPrice:60000,nextSettleTime},market).fundingTime}))));
