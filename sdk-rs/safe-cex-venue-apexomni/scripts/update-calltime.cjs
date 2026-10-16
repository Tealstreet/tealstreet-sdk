const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'trading.cjs');
let code=require('./controlled-exchange-loader.cjs')(fs.readFileSync(filename,'utf8'));
const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,'APEX-UPDATE maintained driver selector');
code=code.replace(driver,()=>String.raw`
(async()=>{
 const rows=[];
 const zk={id:'123456',l2Key:'seed',contractAccount:{makerFeeRate:'0.0001',takerFeeRate:'0.0002'},contractConfig:{perpetualContract:[{symbol:'BTC-USDC',l2PairId:'55'}]}};
 for(const label of ['clear-after-call','fill-after-call']){
  const s=store();s.updateZkLinkConfig(zk);s.markets[0].limits.amount.max=.009;
  requestId=0;signerFailure=false;responses={'/v3/delete-order':{data:{}},'/v3/delete-open-orders':{data:{}}};
  const venue=new Class('apexomni',{key:'key',secret:'secret',passphrase:'pass',omniKeySeed:'seed'},s);
  const mapped=venue.mapOrder({...order,size:'0.006',cumSuccessFillSize:'0'});
  s.addOrUpdateOrdersFull(label==='clear-after-call'?mapped:[]);
  const opts={order:mapped.find(o=>o.id.includes('__take_profit')),update:{price:121}};
  operations=[];
  const promise=venue.updateOrder(opts);
  const prefix={operations:native(operations),cancelled:native([...venue.recentlyCancelledOrders]),orders:native(s.orders)};
  s.addOrUpdateOrdersFull(label==='clear-after-call'?[]:mapped);
  let result;
  try{result={ok:true,value:native(await promise)};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({label,options:native(opts),raw:JSON.stringify(opts),prefix,result,operations:native(operations),cancelled:native([...venue.recentlyCancelledOrders]),orders:native(s.orders)});
 }
 assert.equal(rows.length,2,'APEX-UPDATE independent source floor');
 console.log(JSON.stringify({rows,zk,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
