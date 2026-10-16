const path = require('node:path');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw Error('isolated Axios dependency required');
const axios = require(path.join(path.dirname(isolated), 'axios'));
(async () => {
 const rows=[];
 for (const method of ['get','post','put','patch']) {
  for (const data of [undefined,null,{x:1},[1],false,7,'','raw text','  {"x":1} \n']) {
   for(const json of [false,true]) {
    const client=axios.create({headers:{common:{apikey:'old',...(json?{'Content-Type':'application/json'}:{})}}});
    let signed,wire;
    client.interceptors.request.use(config=>{
     signed={headers:config.headers.toJSON(),data:config.data};
     config.headers.ApiKey='signed';
     return config;
    });
    await client.request({method,url:'/fixture',data,headers:{APIKEY:'request'},adapter:async config=>{
     wire={headers:config.headers.toJSON(),data:config.data};
     return {data:{},status:200,statusText:'OK',headers:{},config};
    }});
    rows.push({method,json,data,signed,wire});
   }
  }
 }
 console.log(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
