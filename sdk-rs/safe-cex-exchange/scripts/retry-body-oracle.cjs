const path=require('node:path');
if(!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('isolated Axios dependencies required');
const root=path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios=require(path.join(root,'axios'));
const retry=require(path.join(root,'axios-retry')).default;
(async()=>{
 const rows=[];
 for(const rewrite of [false,true]) for(const data of [{x:1},'raw text','', '  {"x":1} \n']) {
  const client=axios.create({headers:{'Content-Type':'application/json'}});
  retry(client,{retries:1,retryCondition:()=>true});
  const signs=[],wires=[];
  client.interceptors.request.use(config=>{
   signs.push({data:config.data,timestamp:config.headers['X-Time']??String(1000+signs.length)});
   if(rewrite && signs.length===2) config.data='second raw text';
   config.headers['X-Time']=config.headers['X-Time']??String(999+signs.length);
   return config;
  });
  try {await client.post('/token',data,{adapter:async config=>{
   wires.push(config.data);
   throw new axios.AxiosError('fixture','ECONNRESET',config);
  }});}catch(e){}
  rows.push({data,rewrite,signs,wires});
 }
 console.log(JSON.stringify(rows,null,2));
})().catch(e=>{console.error(e);process.exitCode=1});
