const path = require('node:path');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw Error('isolated Axios dependency required');
const axios = require(path.join(path.dirname(isolated), 'axios'));
(async () => {
 const bodies = [
  {x:'a b',y:2}, {}, [], [1,'a b',null,false],
  {flat:[1,'a b',null,false,'']}, {nested:{x:'a b',nil:null,z:0}},
  {nested:[[1,2],[3]],records:[{x:1},{x:2}]},
  {'raw{}':{z:1,a:[null,false]},'items[]':[{x:1},['a','b']]},
  {' a ':1,' b ':{' c ':2}}, {'nest[]':{x:[1,2]}},
  {'10':'ten','2':'two',z:'last',a:'first'},
  {symbols:'~!*()\' /+é😀',empty:'',nil:null,bool:true},
  {'sparse[]':{'2':'c','0':'a',length:4}},
  {'invalid[]':{length:-1}}, {'bad[]':[{toString:1}]},
 ];
 const nested = depth => { let value={leaf:'a b'}; for(let i=0;i<depth;i++) value={child:value}; return value; };
 bodies.push(nested(100),nested(101),{'meta{}':nested(99)},{'meta{}':nested(100)});
 const rows=[];
 for(const data of bodies) {
  let signed,wire,error;
  const client=axios.create();
  client.interceptors.request.use(config=>{signed=config.data; return config;});
  try {await client.post('/fixture',data,{
   headers:{'Content-Type':'application/x-www-form-urlencoded'},
   adapter:async config=>{wire=config.data;return {data:{},status:200,statusText:'OK',headers:{},config};}
  });} catch(e) {error=e.message;}
  rows.push({data,signed,wire,error});
 }
 console.log(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
