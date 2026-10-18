'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const { v } = require(path.join(root, 'packages/safe-cex/src/utils/get-key.ts'));
function bits(value) {
  assert.equal(typeof value, 'number');
  if (Number.isNaN(value)) return '7ff8000000000000';
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex');
}
const row = (s, value = '-0') => ({ s, c: value, P: '12.5tail', v: '1e400', q: '-Infinity',
  p: value, i: '12.5tail', r: '1e400', T: '-Infinity', b: value, a: '12.5tail' });
const nativeNumbers = {nan:NaN, 'positive-infinity':Infinity, 'negative-infinity':-Infinity, 'negative-zero':-0};
const inputs = [
  ...Object.keys(nativeNumbers).map(nativeNumber=>({events:[row('BTC')],nativeNumber})),
  {events:[]}, {events:[row('BTC')]}, {events:[row('BTC'),row('ETH','99.25')]},
  {events:[row('unknown',{toString:null}),row('BTC')]},
  ...[null,false,[],['-0'],{},'NaN','4.5suffix','-1e400'].map(value=>({events:[row('BTC',value)]})),
  {events:[{s:'BTC'}]},
  {events:[row('BTC',{toString:null}),row('ETH')]},
  ...['capture-error','lookup-error','get-update-error','update-error','truncate','append'].map(mode=>({mode,events:[row('BTC'),row('ETH')],...(mode==='append'?{append:row('BTC','88.75')}:{})})),
];
function observe(exchange, socket, method, input) {
  const events = structuredClone(input.events);
  if(input.nativeNumber) for(const key of ['c','p','b']) events[0][key]=nativeNumbers[input.nativeNumber];
  const trace = [], updates = [], original = new Error('configured ticker callback failure');
  const dictionary = new Proxy({BTC:{id:'BTC'},ETH:{id:'ETH'}},{get(target,key) {
    trace.push(`lookup:${key}`);
    if(input.mode==='lookup-error') throw original;
    return target[key];
  }});
  const store = exchange.store;
  Object.defineProperty(store,'tickersBySymbol',{configurable:true,get() {
    trace.push('capture'); if(input.mode==='capture-error') throw original; return dictionary;
  }});
  let changed = false;
  Object.defineProperty(store,'updateTicker',{configurable:true,get() {
    trace.push('get-update'); if(input.mode==='get-update-error') throw original;
    return (ticker, patch) => {
      trace.push(`update:${ticker.id}`);
      updates.push([ticker.id,Object.entries(patch).map(([key,value])=>[key,bits(value)])]);
      if(!changed) {
        changed = true;
        if(input.mode==='append') events.push(structuredClone(input.append));
        else if(input.mode==='truncate') events.length=0;
      }
      if(input.mode==='update-error') throw original;
    };
  }});
  let error;
  try { socket[method](events); } catch(failure) { error=failure; }
  return {ok:error===undefined,originalError:error===original,trace,updates,length:events.length};
}
const result = {};
for(const [venue,name,family] of [['binance','BinanceExchange','futures'],['binancepm','BinancePMExchange','pm']]) {
  const Exchange = require(path.join(root,`packages/safe-cex/src/exchanges/binance/${venue}.exchange.ts`))[name];
  const store = new DefaultStore();
  const exchange = new Exchange(venue,{publicDataOnly:true,applicationId:'fixture'},store);
  try {
    for(const [channel,method] of [['ticker','handleTickerStreamEvents'],['mark','handleMarkPriceStreamEvents'],['book','handleBookTickersStreamEvents']]) {
      const socket = channel==='book'?exchange.publicWebsocket:exchange.publicWebsocket.marketWebsocket;
      result[`handle_${family}_${channel}_events`] = inputs.map(input=>({input,output:observe(exchange,socket,method,input)}));
    }
  } finally { exchange.dispose(); }
}
const fields = [null,false,[],['-0'],{},'NaN','4.5suffix','-0','-1e400',17.25,{toString:null}].map(value=>({event:{c:value},key:'c'}));
fields.push({event:{},key:'c'},{event:null,key:'c'},{event:{funding_rate:'2.5'},key:'fundingRate'},
  {event:{fundingRate:'-0',funding_rate:'9'},key:'fundingRate'}, {event:{},key:'toString'});
fields.push(...Object.keys(nativeNumbers).map(nativeNumber=>({event:{},key:'c',nativeNumber})));
result.parse_field = fields.map(input=>{
  const event = structuredClone(input.event);
  if(input.nativeNumber) event.c=nativeNumbers[input.nativeNumber];
  try { return {input,output:{ok:true,bits:bits(parseFloat(v(event,input.key)))}}; }
  catch(error) { return {input,output:{ok:false,name:error.name}}; }
});
assert.equal(Object.keys(result).length,7);
assert.equal(inputs.length,24);
process.stdout.write(JSON.stringify(result));
