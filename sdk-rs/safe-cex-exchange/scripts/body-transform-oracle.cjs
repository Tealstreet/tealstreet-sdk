const path = require('node:path');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw Error('isolated Axios dependency required');
const axios = require(path.join(path.dirname(isolated), 'axios'));
const values = ['raw text', '', '{"x":1}', '  {"x":1} \n', '"text"', 'null', '1e400', '\ufeff{"x":1}', '\u0085null', '[1,]', '{"x":"\\ud800"}', '['.repeat(200)+'0'+']'.repeat(200)];
(async () => {
 const rows=[];
 for(const contentType of [null,'application/json','application/json;charset=utf-8','APPLICATION/JSON','text/plain']) {
  for(const data of values) {
   const client=axios.create(); let signed, wire;
   client.interceptors.request.use(config=>{signed=config.data; return config;});
   await client.post('/fixture',data,{headers:contentType===null?{}:{'cOnTeNt-TyPe':contentType},adapter:async config=>{
    wire=config.data; return {data:{},status:200,statusText:'OK',headers:{},config};
   }});
   rows.push({data,contentType,signed,wire});
  }
 }
 console.log(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
