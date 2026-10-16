const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH))
  : require;
const axios=runtimeRequire('axios');
const createHmac=runtimeRequire('create-hmac');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/kucoin/kucoin.broker.ts'),'utf8');
const moduleObject={exports:{}};
const code=ts.transpileModule(source+'\nexport {trimString,getHeader};',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
new Function('module','exports','require',code)(moduleObject,moduleObject.exports,name=>{if(name==='axios')return axios;if(name==='create-hmac')return createHmac;throw Error(name);});
const api=moduleObject.exports;
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(cases.map(({options,headers,timestamp,trim})=>({
 trim:api.trimString(trim)??null,
 config:api.getKucoinBrokerConfig(options)??null,
 timestamp:api.getKucoinRequestTimestamp(headers)??null,
 headers:api.addKucoinBrokerHeaders(new axios.AxiosHeaders(headers),options,timestamp).toJSON(),
}))));
