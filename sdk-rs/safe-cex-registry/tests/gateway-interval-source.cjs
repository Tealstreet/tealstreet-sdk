const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const intervals = ['1M','1s','5s','15s','30s','1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','3h','5h','7h','8h','9h','10h','11h','24h','2d','3d'];
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/external-public-data.ts');
const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const moduleObject={exports:{}};
class Socket { static OPEN=1; }
vm.runInNewContext(code,{module:moduleObject,exports:moduleObject.exports,WebSocket:Socket,
  setTimeout(){return 1;},clearTimeout(){},require(name){
    if(name.includes('jsonParse'))return {jsonParse:JSON.parse};
    if(name.includes('ReconnectingWebsocket'))return {ReconnectingWebSocket:Socket};
    if(name==='../config/config')return {ENABLE_RECONNECTING_WEBSOCKET:false};
    if(name==='../types')return {LogSeverity:{Info:'info',Warning:'warning'}};
    throw Error(`unexpected source import ${name}`);
  }});
for(const interval of intervals){
 const frames=[];
 const provider=new moduleObject.exports.ExternalPublicDataProvider({log(){}},{url:'wss://fixture',exchangeId:'bybit'});
 provider.ws={readyState:1,send(text){frames.push(JSON.parse(text));},close(){}};
 const seen=[];
 const off=provider.subscribeCandle({symbol:'BTC:PERP',interval},candle=>seen.push(candle));
 provider.resubscribeAll();
 provider.handleDataMessage({channel:'candle',symbol:'BTC:PERP',interval,data:{timestamp:123}});
 off();
 assert.deepEqual(frames.map(frame=>[frame.type,frame.symbol,frame.interval]),[['subscribe','BTC:PERP',interval],['subscribe','BTC:PERP',interval],['unsubscribe','BTC:PERP',interval]],'GATEWAY-INTERVAL source subscribe/reconnect/unsubscribe preserve interval');
 assert.equal(seen.length,1);assert.equal(seen[0].timestamp,123);
}
console.log('GATEWAY-INTERVAL actual source: 27 subscribe/reconnect/delivery/unsubscribe cases PASS');
