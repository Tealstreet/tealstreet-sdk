// Full maintained classes; only terminal Axios I/O is scripted. No venue method
// or Store operation is replaced. Inputs accompany observations for native replay.
const assert = require('node:assert/strict');
const path = require('node:path');
const {root, load} = require('./inverse-source-loader.cjs');
const {BinanceInverseExchange} = load(path.join(root, 'packages/safe-cex/src/exchanges/binance/binanceinverse.exchange.ts'));
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const symbol = 'BTCUSD_PERP';
const market = {id:'BTC/USD:BTC',symbol,base:'BTC',quote:'USD',active:true,precision:{amount:1,price:0.1},limits:{amount:{min:1,max:10},leverage:{min:1,max:50}},contractSize:100,isInverse:true};
const ticker = {id:market.id,symbol,bid:4999,ask:5001,last:5000,mark:5000,index:5000,percentage:0,fundingRate:0,fundingTime:0,volume:1,quoteVolume:5000,openInterest:0};
const order = (id, sym = symbol) => ({id:String(id),symbol:sym,type:'limit',side:'buy',status:'open',price:5000,amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:1700000000000});
const position = (side, contracts = 0) => ({id:symbol+':'+side,symbol,side,contracts,entryPrice:4000,notional:0,leverage:10,unrealizedPnl:0,liquidationPrice:0,marginSize:0,positionMode:'hedged',marginType:'cross'});
const opts = extra => ({symbol,side:'buy',type:'limit',amount:2,price:5000.16,...extra});
const route = (method, url, data) => ({method,url,data});
const single = route('POST','/dapi/v1/order',{orderId:7});
const batch = route('POST','/dapi/v1/batchOrders',[{orderId:1},{code:-1,msg:'batch rejected'},{orderId:null},{orderId:0},{orderId:5}]);
const cancel = route('DELETE','/dapi/v1/order',{});
const cases = [];
function add(name, method, args, routes, extra = {}) { cases.push({name,method,args,routes,seed:{markets:[market],tickers:[ticker],orders:[],positions:[],...extra.seed},hedged:extra.hedged || false}); }
add('limit','placeOrder',[opts({})],[single]);
add('market','placeOrder',[opts({type:'market'})],[single]);
add('split-attached','placeOrder',[opts({amount:26,stopLoss:4500.16,takeProfit:5500.16,slTriggerType:'last_price',tpTriggerType:'mark_price'})],[batch,single]);
add('hedged-attached','placeOrder',[opts({stopLoss:4500,takeProfit:5500})],[batch],{hedged:true});
add('trailing-min','placeOrder',[opts({type:'trailing_stop_market',price:4999})],[single]);
add('trailing-max','placeOrder',[opts({type:'trailing_stop_market',price:1000})],[single]);
add('full-close','placeOrder',[opts({type:'stop_market',amount:0,price:4500,close:true})],[single],{seed:{positions:[position('short',2)]}});
add('full-close-missing','placeOrder',[opts({type:'stop_market',amount:0,close:true})],[]);
add('format-all-before-send','placeOrders',[[opts({}),opts({symbol:'MISSING'})]],[]);
const grouped = [...Array.from({length:11},(_,i)=>order(i+1)),order(20,'ETHUSD_PERP'),order(21,'ETHUSD_PERP')];
add('cancel-partial-unknown','cancelOrders',[grouped],[route('DELETE','/dapi/v1/batchOrders',{}),route('DELETE','/dapi/v1/order',{code:-2011,msg:'Unknown order'})],{seed:{orders:grouped}});
const unusualGroups=[order(30,'10'),order(31,'2'),order(32,'__proto__'),order(33,'10'),order(34,'2'),order(35)];
add('cancel-group-order','cancelOrders',[unusualGroups],[route('DELETE','/dapi/v1/batchOrders',{}),cancel],{seed:{orders:unusualGroups}});
add('cancel-symbol','cancelSymbolOrders',[symbol],[route('DELETE','/dapi/v1/allOpenOrders',{})],{seed:{orders:[order(1),order(2,'ETHUSD_PERP')]}});
add('amend-zero-price','updateOrder',[{order:order(1),update:{price:0,amount:3}}],[cancel,single],{seed:{orders:[order(1)]}});
add('amend-cancel-failed','updateOrder',[{order:order(1),update:{amount:3}}],[route('DELETE','/dapi/v1/order',{code:-4000,msg:'cancel rejected'})],{seed:{orders:[order(1)]}});
add('amend-unknown-order','updateOrder',[{order:order(1),update:{amount:3}}],[route('DELETE','/dapi/v1/order',{code:-2011,msg:'Unknown order'}),single],{seed:{orders:[order(1)]}});
for (const [name,value] of [['min',-5],['max',100],['unchanged',10]]) add('leverage-'+name,'setLeverage',[symbol,value],[route('POST','/dapi/v1/leverage',{})],{seed:{positions:[position('long'),position('short')]}});
add('leverage-failure','setLeverage',[symbol,20],[route('POST','/dapi/v1/leverage',{code:-4000,msg:'leverage rejected'})],{seed:{positions:[position('long')]}});
add('leverage-missing','setLeverage',[symbol,20],[]);
add('mode-confirmed','changePositionMode',[true,symbol],[route('POST','/dapi/v1/positionSide/dual',{})],{seed:{positions:[position('long'),position('short')]}});
add('mode-already-set','changePositionMode',[false,symbol],[route('POST','/dapi/v1/positionSide/dual',{code:-4059,msg:'No need to change position side.'})],{hedged:true,seed:{positions:[position('long'),position('short')]}});
add('mode-occupied','changePositionMode',[true],[route('POST','/dapi/v1/positionSide/dual',{})],{seed:{positions:[position('long',1)]}});
add('margin-type','changeMarginType',[true,symbol],[route('POST','/dapi/v1/marginType',{})]);
add('margin-failure','changeMarginType',[false,symbol],[route('POST','/dapi/v1/marginType',{code:-4000,msg:'margin rejected'})]);

function normalize(value) {
  if (Array.isArray(value)) return value.map(normalize);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => {
    if (k === 'newClientOrderId') { assert.match(v,/^x-s3vFFRQp.+/); return [k,'x-s3vFFRQp<generated>']; }
    return [k,normalize(v)];
  }));
  return value;
}
function numbers(value) {
  if (typeof value === 'number') { const bytes=Buffer.alloc(8); bytes.writeDoubleBE(value); return {$f64:bytes.toString('hex')}; }
  if (Array.isArray(value)) return value.map(numbers);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,numbers(v)]));
  return value;
}
(async () => {
  const outputs=[];
  for (const input of cases) {
    const spec=structuredClone(input);
    const store=new DefaultStore();
    const venue=new BinanceInverseExchange('binanceinverse',{key:'synthetic-key',secret:'synthetic-secret'},store);
    store.update(structuredClone(spec.seed)); store.setSetting('isHedged',spec.hedged);
    const calls=[],events=[];
    venue.on('error',v=>events.push(v));
    for (const client of new Set([venue.xhr,venue._primaryXhr,venue._secondaryXhr,venue.publicXhr,venue.unlimitedXHR])) client.defaults.adapter=async config=>{
      const matched=spec.routes.find(r=>r.method.toLowerCase()===config.method && r.url===config.url);
      assert.ok(matched,`unscripted ${spec.name} ${config.method} ${config.url}`);
      assert.equal(config.headers.get('X-MBX-APIKEY'),'synthetic-key');
      assert.equal(config.data,undefined);
      assert.match(config.params.signature,/^[a-f0-9]{64}$/);
      assert.equal(typeof config.params.timestamp,'number');
      const params=Object.entries(config.params).filter(([k])=>!['timestamp','signature'].includes(k)).map(([k,v])=>[k,k==='batchOrders'?normalize(JSON.parse(v)):k==='newClientOrderId'?normalize({[k]:v})[k]:String(v)]);
      calls.push({method:config.method.toUpperCase(),path:config.url,params});
      return {status:200,statusText:'OK',headers:{'content-type':'application/json'},config,data:JSON.stringify(matched.data)};
    };
    try {
      let result=null,error=null;
      try { result=(await venue[spec.method](...structuredClone(spec.args))) ?? null; } catch(e) { error=e.message; }
      const state={orders:store.orders.map(o=>({id:o.id,status:o.status})),positions:store.positions.map(p=>({id:p.id,leverage:p.leverage,positionMode:p.positionMode})),isHedged:store.options.isHedged,fetchedLeverage:store.fetchedLeverage};
      outputs.push({...spec,expected:numbers({result,error,calls,events,state})});
    } finally { venue.dispose(); }
  }
  assert.equal(outputs.length,25);
  const observation=name=>outputs.find(c=>c.name===name).expected;
  const params=(name,index=0)=>Object.fromEntries(observation(name).calls[index].params);
  assert.equal(params('limit').price,'5000.2');
  assert.equal(params('market').price,undefined);
  assert.deepEqual(params('split-attached').batchOrders.map(p=>p.quantity),['7','7','7','6','26']);
  assert.deepEqual(observation('split-attached').result,['1','0','5','7']);
  assert.equal(params('hedged-attached').batchOrders[1].positionSide,'LONG');
  assert.equal(params('hedged-attached').batchOrders[1].reduceOnly,undefined);
  assert.equal(params('trailing-min').callbackRate,'0.1');
  assert.equal(params('trailing-max').callbackRate,'10');
  assert.equal(params('full-close').closePosition,'true');
  assert.equal(params('full-close').quantity,undefined);
  assert.equal(observation('format-all-before-send').calls.length,0);
  assert.deepEqual(observation('cancel-partial-unknown').state.orders.map(o=>o.status),[...Array(10).fill('canceled'),'open','open','open']);
  assert.equal(observation('cancel-partial-unknown').calls.length,2);
  assert.deepEqual(observation('cancel-group-order').calls.map(c=>Object.fromEntries(c.params).symbol),['2','10','__proto__',symbol]);
  assert.ok(observation('cancel-group-order').state.orders.every(o=>o.status==='canceled'));
  assert.deepEqual(observation('amend-zero-price').calls.map(c=>c.method),['DELETE','POST']);
  assert.equal(params('amend-zero-price',1).price,'undefined');
  assert.equal(params('amend-zero-price',1).quantity,'3');
  assert.equal(observation('amend-cancel-failed').calls.length,1);
  assert.deepEqual(observation('amend-unknown-order').calls.map(c=>c.method),['DELETE','POST']);
  assert.equal(params('leverage-min').leverage,'1');
  assert.equal(params('leverage-max').leverage,'50');
  assert.equal(observation('leverage-unchanged').calls.length,0);
  assert.equal(observation('mode-already-set').state.isHedged,false);
  assert.equal(observation('mode-already-set').state.positions.length,1);
  assert.equal(observation('mode-occupied').calls.length,0);
  assert.equal(params('margin-type').marginType,'ISOLATED');
  assert.equal(observation('margin-failure').error,'margin rejected');
  process.stdout.write(JSON.stringify(outputs));
})().catch(error=>{console.error(error);process.exitCode=1;});
