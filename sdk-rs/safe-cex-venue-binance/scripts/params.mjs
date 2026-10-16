import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(new URL('../../../packages/safe-cex/src/exchanges/binance/binance.utils.ts',import.meta.url),'utf8');
const exports = {};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const rows=[];
const limits=[undefined,null,-100,-0,0,.5,1,50.5,1000,1001,1e30];
for (const limit of limits) for (const orderId of [undefined,null,'','0','  ','9007199254740993','\ufeff'])
for (const from of [undefined,0,123]) for (const to of [undefined,0,456])
for (const fromId of [undefined,0,-1,1.5,123,9007199254740991]) {
 const input={symbol:'BTCUSDT',limit,orderId,from,to,fromId,accountName:'ignored'};
 rows.push({input,expected:exports.buildUserTradesParams(input)});
}
for (const limit of [NaN,Infinity,-Infinity]) {
 const input={symbol:'BTCUSDT',limit};
 const actual=exports.buildUserTradesParams(input);
 rows.push({special:String(limit),expected:String(actual.limit)});
}
process.stdout.write(JSON.stringify({maxLimit:exports.USER_TRADES_MAX_LIMIT,maxWindow:exports.USER_TRADES_MAX_WINDOW_MS,rows}));
