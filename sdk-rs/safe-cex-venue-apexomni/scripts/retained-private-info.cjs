const assert=require('node:assert/strict');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const {ApexOmniExchange}=require(path.join(root,'packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts'));
const {DefaultStore}=require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const store=new DefaultStore();
store.update({markets:[{id:'BTC-USDC',symbol:'BTCUSDC',base:'BTC',quote:'USDC',active:true,precision:{amount:0.001,price:0.01},limits:{amount:{min:0.001,max:1000},leverage:{min:1,max:50}},info:{initialMarginRate:'0.1'}}],positions:[{id:'BTC-USDC:long',symbol:'BTCUSDC',side:'long',contracts:0,entryPrice:0,leverage:10,notional:0,unrealizedPnl:0,liquidationPrice:0}]});
const venue=new ApexOmniExchange('apexomni',{key:'fixture-key',secret:'fixture-secret'},store);
try {
  const order={id:'private-retained-order',symbol:'BTC-USDC',type:'LIMIT',side:'BUY',status:'OPEN',timeInForce:'GOOD_TIL_CANCEL',price:'100.12',size:'1.234',cumSuccessFillSize:'0',reduceOnly:false,triggerPrice:'0',openSlParams:{triggerPrice:'0'},openTpParams:{triggerPrice:'0'},opaque:'OPAQUE_WIRE_VALUE'};
  const raw=JSON.stringify({topic:'ws_zk_accounts_v3',contents:{orders:[order],positions:[{symbol:'BTC-USDC',side:'LONG',size:'4',entryPrice:'123',customImr:'0.1',opaque:'OPAQUE_WIRE_VALUE'}]}}).replaceAll('"OPAQUE_WIRE_VALUE"','{"text":"\\ud800","\\udc00":"opaque-key-value","overflow":1e400,"negativeZero":-0}');
  venue.privateWebsocket.onMessage({data:raw});
  const rows=[store.orders.find(o=>o.id===order.id),store.positions.find(p=>p.contracts===4)];
  const capture=rows.map(row=>{
    assert.ok(row,'actual private handler must update real Store');
    const opaque=row.info.opaque;
    assert.equal(opaque.text.charCodeAt(0),0xd800);
    assert.equal(opaque['\udc00'],'opaque-key-value');
    assert.equal(opaque.overflow,Infinity);
    assert.ok(Object.is(opaque.negativeZero,-0));
    return {textUnits:[opaque.text.charCodeAt(0)],keyUnits:[Object.keys(opaque).find(k=>k.charCodeAt(0)===0xdc00).charCodeAt(0)],overflow:'infinity',negativeZero:true};
  });
  process.stdout.write(JSON.stringify(capture));
} finally {venue.dispose();}
