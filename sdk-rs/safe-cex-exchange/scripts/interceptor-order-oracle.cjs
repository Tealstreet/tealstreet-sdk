const path = require('node:path');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw Error('isolated Axios dependency required');
const axios = require(path.join(path.dirname(isolated), 'axios'));
(async()=>{
 const rows=[];
 for(const mode of ['clear','replace','reject']) {
  const client=axios.create({baseURL:'https://factory.example',timeout:20000,headers:{'x-default':'factory'}});
  let before,wire,total=0;
  client.interceptors.request.use(config=>{total++;return config;});
  client.interceptors.request.use(async config=>{
   before={headers:config.headers.toJSON(),baseURL:config.baseURL,timeout:config.timeout};
   await Promise.resolve();
   if(mode==='reject') throw Error('interceptor refused');
   config.headers=mode==='clear'?{'x-site-id':'kline'}:{accept:'custom','x-site-id':'kline'};
   config.baseURL=undefined;config.timeout=undefined;
   return config;
  });
  try {
   await client.request({method:'get',url:'/kline',adapter:async config=>{
    wire={headers:config.headers.toJSON(),baseURL:config.baseURL,timeout:config.timeout};
    return {data:{},status:200,statusText:'OK',headers:{},config};
   }});
  } catch(error) { if(error.message!=='interceptor refused') throw error; }
  rows.push({mode,before,wire,total});
 }
 console.log(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
