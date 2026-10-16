// Whole venue interceptors plus actual Axios and the production remote adapter.
const {capture}=require('./api-retry-capture.cjs');
(async()=>{
 const rows=[];
 for(const venue of ['kucoin','kucoinspot']){
  const scenarios=[
   {data:{symbol:'BTC USDT'}}, {data:null}, {data:false}, {data:0}, {data:''},
   {data:'raw text'}, {data:'  false  '}, {data:' {"x":1} '},
   {data:{x:1},request:{headers:{'kc-api-key':'override','content-type':'application/json','accept':'custom'}}},
   {data:{x:1},request:{headers:{'KC-API-TIMESTAMP':'1700000000099'}}},
   {data:{x:1},options:{publicDataOnly:true}},
   {data:{x:1},url:'/api/v2/symbols-extra'},
   {data:{x:1},options:{extra:{recvWindow:0.5}}},
   {data:{x:1},options:{extra:{recvWindow:-1}}},
   {data:{x:'a b',y:2},request:{headers:{'Content-Type':'application/x-www-form-urlencoded'}},finding:'form-body'},
  ];
  for(const scenario of scenarios){const {data,url='/api/v1/orders',...extra}=scenario;rows.push(await capture(venue,'post',url,data,'network',0,{...extra,failures:0,extended:true}));}
  for(const data of [false,true,0,1,'raw text','  false  ',' {"x":1} ',{x:1},null])rows.push(await capture(venue,'post','/api/v1/orders',data,'network',0,{remote:true,failures:0,extended:true}));
 }
 console.log(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
