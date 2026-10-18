// Execute the full maintained class, Store, API and socket lifecycle. Only the
// terminal Axios adapter and never-opening WebSocket transport are supplied.
const assert=require('node:assert/strict');
const path=require('node:path');
// Keep the native console sink observable without mixing logs into the capture.
console.log=(...args)=>console.error(...args);
class HeldSocket extends EventTarget {
  static CONNECTING=0; static OPEN=1; static CLOSING=2; static CLOSED=3;
  readyState=0; bufferedAmount=0;
  constructor(url) { super(); this.url=url; }
  send() { throw Error('held socket cannot send before open'); }
  close() { this.readyState=3; }
}
globalThis.WebSocket=HeldSocket;
const root=path.resolve(__dirname,'../../..');
const {ApexOmniExchange}=require(path.join(root,'packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts'));
const {DefaultStore}=require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const contract={symbol:'BTC-USDC',crossSymbolName:'BTCUSDC',baseTokenId:'BTC',settleAssetId:'USDC',l2PairId:'11',enableTrade:true,stepSize:'0.001',tickSize:'0.01',minOrderSize:'0.001',maxOrderSize:'1000',displayMinLeverage:'1',displayMaxLeverage:'50',initialMarginRate:'0.1'};
const account={id:'16909060',l2key:'fixture-l2',positions:[{symbol:'BTC-USDC',side:'LONG',size:'4',entryPrice:'123',customImr:'0.1'}],contractAccount:{makerFeeRate:'0.0012',takerFeeRate:'0.0013'}};
const drain=()=>new Promise(resolve=>setImmediate(resolve));
(async()=>{
  const rows=[];
  for (const disposed of [false,true]) {
    const store=new DefaultStore();
    const venue=new ApexOmniExchange('apexomni',{key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-passphrase',extra:{tickInterval:60000}},store);
    const calls=[],pending=[];
    let releaseFirst;
    for (const client of [venue.xhr,venue.unlimitedXHR]) client.defaults.adapter=config=>{
      calls.push(config.url);
      const response=body=>({status:200,statusText:'OK',headers:{'content-type':'application/json'},config,data:JSON.stringify(body)});
      if (config.url==='/v3/symbols') return Promise.resolve(response({code:0,data:{contractConfig:{perpetualContract:[contract]}}}));
      if (config.url==='/v3/account-balance') return Promise.resolve(response({code:10001,msg:'startup balance rejected'}));
      assert.ok(['/v3/account','/v3/open-orders'].includes(config.url),'unexpected terminal '+config.url);
      return new Promise(resolve=>{
        const release=()=>resolve(response({code:0,data:config.url==='/v3/account'?account:[]}));
        if (config.url==='/v3/account' && !releaseFirst) releaseFirst=release;
        else pending.push(release);
      });
    };
    try {
      await venue.start();
      // Start's catch has finished; its separate tick must really start too.
      for(let i=0;i<20 && calls.filter(p=>p==='/v3/account').length<2;i++) await drain();
      assert.equal(calls.filter(p=>p==='/v3/account').length,2);
      assert.equal(calls.filter(p=>p==='/v3/open-orders').length,1);
      assert.equal(store.positions.length,0);
      assert.equal(store.loaded.positions,false);
      assert.equal(store.loaded.orders,false);
      if(disposed) venue.dispose();
      releaseFirst();
      for(let i=0;i<8;i++) await drain();
      const row={disposed,contracts:store.positions.find(p=>p.side==='long')?.contracts ?? null,positions:store.loaded.positions,orders:store.loaded.orders,balance:store.loaded.balance,accounts:calls.filter(p=>p==='/v3/account').length};
      assert.deepEqual(row,{disposed,contracts:disposed?null:4,positions:!disposed,orders:!disposed,balance:false,accounts:2});
      rows.push(row);
    } finally { venue.dispose(); }
    // Leave tick's independent terminal requests unresolved: settling them would
    // introduce another snapshot and mask which continuation wrote the Store.
  }
  process.stdout.write(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
