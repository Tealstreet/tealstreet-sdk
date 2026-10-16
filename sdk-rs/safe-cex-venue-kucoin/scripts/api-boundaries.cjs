// Observable representation boundaries of the two real factory interceptors.
const {capture}=require('./api-retry-capture.cjs');
(async()=>{
 const rows=[];
 for(const venue of ['kucoin','kucoinspot']) {
  const extra={failures:0,extended:true,options:{extra:{kucoinBroker:{partner:'before',brokerKey:'fixture-broker-key',brokerName:'fixture-broker'}}}};
  const before=JSON.parse(JSON.stringify(extra));
  const pending=capture(venue,'post','/api/v1/orders',{x:1},'network',0,extra);
  extra.options.extra.kucoinBroker.partner='after';
  const alias=await pending;
  rows.push({kind:'alias',before,after:extra,case:alias});
  for(const afterCreate of [{key:'changed-key'},{publicDataOnly:true}]) {
   rows.push({kind:'option-capture',case:await capture(venue,'post','/api/v1/orders',{x:1},'network',0,{failures:0,extended:true,afterCreate})});
  }
  for(const value of [false,'false']) {
   rows.push({kind:'header',value,case:await capture(venue,'post','/api/v1/orders',{x:1},'network',0,{failures:0,extended:true,request:{headers:{'KC-API-TIMESTAMP':value}}})});
  }
  for(const value of ['ascii','🔥',' \uFEFFx ','\ud800','\udfff']) {
   const result=await capture(venue,'post','/api/v1/orders',value,'network',0,{failures:0,extended:true});
   result.data=null; // retain lone UTF-16 surrogates only in the double-escaped witness
   // The signed string body reaches the preimage; Node hashes it as its well-formed (U+FFFD) form.
   for(const hash of result.hashes) hash.preimage=hash.preimage.toWellFormed();
   rows.push({kind:'string',rawData:JSON.stringify(value),refused:value==='\ud800'||value==='\udfff',case:result});
  }
 }
 process.stdout.write(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
