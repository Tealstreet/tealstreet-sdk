const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
let source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/utils/axiosErrorEnhancer.ts'),'utf8');
if(process.env.SAFE_CEX_AXIOS_ORACLE_NEGATIVE_CONTROL==='timeout') {
 const target="context.isTimeout = true;";
 if(!source.includes(target)) throw Error('timeout source target disappeared');
 source=source.replace(target,'context.isTimeout = false;');
}
if(process.env.SAFE_CEX_AXIOS_ORACLE_NEGATIVE_CONTROL==='network') {
 const target="error.code !== 'ECONNABORTED'";
 if(!source.includes(target)) throw Error('network source target disappeared');
 source=source.replace(target,'false');
}
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const cases=[];
for(const isError of [false,true]) for(const xhr of [false,true]) for(const shape of [null,'oops',{}, {message:'Network Error',request:{}}, ...['ECONNABORTED','ERR_NETWORK','ETIMEDOUT','ENOTFOUND','ECONNREFUSED','ECONNRESET','OTHER',''].map(code=>({message:'failure',code,request:{},config:{url:'/api',method:'post'}})),{message:'failure',response:{status:401,statusText:'Unauthorized',data:{msg:'',retMsg:'bad key'}},config:{url:'/private',method:'get'}},{message:'failure',response:{},config:{}},{message:'failure',code:'ERR_NETWORK',isAxiosError:true}, {message:'Network Error',code:'OTHER',isAxiosError:true}]) {
 class XHR{};
 const exports={};const timestamp='2026-10-12T00:00:00.000Z';
 class Clock extends Date {constructor(){super(timestamp)}}
 new Function('exports','require','Date','window','XMLHttpRequest',compiled)(exports,()=>({}),Clock,xhr?{}:undefined,xhr?XHR:undefined);
 const error=isError?Object.assign(new Error(shape?.message),shape):shape;
 if(xhr&&error?.request) error.request=new XHR();
 const input=isError?{...error,message:error.message}:shape;let result,threw=false;
 try {const enhanced=exports.enhanceAxiosError(error,'venue');result={message:enhanced.message,enhancedMessage:enhanced.enhancedMessage,context:enhanced.context,originalMessage:enhanced.originalError?.message};} catch {threw=true;}
 cases.push({input,isError,xhr,timestamp,threw,result,network:exports.isNetworkError(error)});
}
console.log(JSON.stringify({cases}));
