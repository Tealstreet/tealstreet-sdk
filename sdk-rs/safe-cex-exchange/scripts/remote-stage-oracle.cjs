const fs=require('node:fs'),path=require('node:path'),{createRequire}=require('node:module');
const isolated=process.env.SAFE_CEX_TYPESCRIPT_PATH;
if(!isolated) throw Error('isolated remote signer dependencies required');
const deps=createRequire(path.join(path.dirname(isolated),'axios','package.json'));
const ts=deps('typescript'),axios=deps('axios');
const root=path.resolve(__dirname, '../../..');const cache=new Map();
function load(file){
 file=path.resolve(file);if(cache.has(file))return cache.get(file).exports;
 const module={exports:{}};cache.set(file,module);
 const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 new Function('require','module','exports',code)(name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):deps(name),module,module.exports);
 return module.exports;
}
(async()=>{
 const {createRemoteSigningAdapter}=load(path.join(root,'packages/safe-cex/src/remoteSigningAdapter.ts'));
 const {tryParseJson}=load(path.join(root,'packages/safe-cex/src/utils/tryParseJson.ts'));
 const rows=[];
 for (const mode of ['browser','echo']) {
 for(const contentType of [undefined,'application/json','application/x-www-form-urlencoded']){
 for(const data of [undefined,null,false,7,'','raw text','  {"x":1} \n',{x:'a b'},[1,2]]){
  let remote,clientInput,wire;
  const signer={sign:async request=>{
   remote=request.config;
   if (mode === 'echo') return {id:request.id,config:request.config};
   const client=axios.create();
   client.interceptors.request.use(config=>{clientInput={data:config.data,headers:config.headers.toJSON()};return config;});
   let signed;
   await client.request({...request.config,data:tryParseJson(request.config.data),params:tryParseJson(request.config.params),adapter:async config=>{
    signed={url:config.url,data:config.data,params:config.params,headers:config.headers.toJSON()};
    return {data:{},status:200,statusText:'OK',headers:{},config};
   }});
   return {id:request.id,config:signed};
  }};
  const adapter=createRemoteSigningAdapter(signer,'kucoin',()=>async config=>{
   wire={data:config.data,headers:config.headers.toJSON()};
   return {data:{},status:200,statusText:'OK',headers:{},config};
  });
  await axios.request({method:'post',baseURL:'https://example.test',url:'/data',data,headers:contentType?{'Content-Type':contentType}:{},adapter});
  rows.push({mode,contentType,data,remote,clientInput,wire});
 }}
 }
 console.log(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
