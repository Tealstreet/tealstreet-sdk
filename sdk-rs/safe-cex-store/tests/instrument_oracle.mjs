import assert from 'node:assert/strict';
import {loadSource} from './source_loader.mjs';
import fs from 'node:fs';
const {classifyMarket, isNonCryptoInstrument} = loadSource(new URL('../../../packages/safe-cex/src/instrument-class/resolve.ts', import.meta.url));
const {describeInstrument, tradfiPreflightMessage} = loadSource(new URL('../../../packages/safe-cex/src/instrument-class/describe.ts', import.meta.url));
const cases = [];
for (const exchange of ['bybit','bybitdemo','bybitinverse','bybitspot','hyperliquid','binance']) {
  for (const info of [null, [], {}, {symbolType:null}, {symbolType:false}, {symbolType:42}, {symbolType:[]}, ...['','innovation','stock','ETF','commodity','forex','new-class','constructor','__proto__','toString','\uFEFFstock\uFEFF','\u0085stock\u0085'].map(symbolType=>({symbolType,underlyingTicker:'\uFEFFShopify\uFEFF',marketRegion:' US '}))]) {
    const result = classifyMarket(exchange, {info});
    cases.push({exchange,info,nonCrypto:isNonCryptoInstrument(result) ?? null,description:describeInstrument('X',exchange,result),preflight:tradfiPreflightMessage('X',exchange,result) ?? null});
  }
}
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(cases.length, 114);
const path = new URL('./instrument_oracle.json', import.meta.url);
const output = JSON.stringify(cases,null,2)+'\n';
if (process.argv.includes('--check')) {
  if (fs.readFileSync(path,'utf8') !== output) throw new Error('instrument oracle fixtures changed');
  console.log(`${cases.length} instrument source-oracle cases verified`);
} else fs.writeFileSync(path,output);
