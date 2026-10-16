const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const dir=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const protocol={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(dir,'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
const output={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(dir,'coinbase.starbase-public-frames.ts'),'utf8')+'\nexport { classifyNotification, classifyPublicTestResponse };',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:output,require:()=>protocol});
const input=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../tests/fixtures/frame-classification.json'),'utf8'));
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')};
const pack=v=>{
 const tokens=[];
 const visit=v=>{
  if(v===null){tokens.push({type:'null'});return}
  if(typeof v==='boolean'){tokens.push({type:'bool',value:v});return}
  if(typeof v==='number'){tokens.push({type:'number',bits:bits(v)});return}
  if(typeof v==='string'){tokens.push({type:'string',units:units(v)});return}
  if(Array.isArray(v)){tokens.push({type:'array',length:v.length});v.forEach(visit);return}
  const keys=Object.keys(v);tokens.push({type:'object',keys:keys.map(units)});keys.forEach(key=>visit(v[key]));
 };
 visit(v);return tokens;
};
const project=v=>{
 switch(v.kind){
  case 'public-subscription':return{kind:v.kind,channel:v.channel,payload:{channel:v.payload.channel,data:pack(v.payload.data)}};
  case 'public-test-response':return v.outcome==='result'?{kind:v.kind,idBits:bits(v.id),outcome:v.outcome,version:units(v.result.version)}:{kind:v.kind,idBits:bits(v.id),outcome:v.outcome,error:{codeBits:bits(v.error.code),message:units(v.error.message),hasData:Object.hasOwn(v.error,'data'),...Object.hasOwn(v.error,'data')?{data:pack(v.error.data)}:{}}};
  case 'unsupported-subscription':return{...v,channelUnits:units(v.channel),channel:undefined};
  default:return v;
 }
};
const capture=fn=>{try{return{ok:project(fn())}}catch(e){return{error:{name:e.name,message:e.message}}}};
const notifications=input.notifications.map(c=>capture(()=>output.classifyNotification(c.frame,new Set(c.ack),new Set(c.desired))));
const responses=input.responses.map(c=>capture(()=>output.classifyPublicTestResponse(c.frame,new Set(c.pending))));
const raw=input.raw.map(c=>capture(()=>output.classifyCoinbaseStarbasePublicWebsocketFrame(c.units===null?null:String.fromCharCode(...c.units),c.context)));
const channel='ticker.BTC_USDC-PERPETUAL.100ms';
const aliases=[];
for(const data of [{marker:1},[1],3]){
 const frame={jsonrpc:'2.0',method:'subscription',params:{channel,data}};
 const result=output.classifyNotification(frame,new Set([channel]),new Set([channel]));
 if(Array.isArray(data)){result.payload.data.push(88);aliases.push({kind:'array',same:result.payload.data===data,input:pack(data)})}
 else if(typeof data==='object'){result.payload.data.added=88;aliases.push({kind:'object',same:result.payload.data===data,input:pack(data)})}
 else{result.payload.data=99;aliases.push({kind:'primitive',input:pack(frame.params.data)})}
}
const errorData={marker:1},frame={jsonrpc:'2.0',id:7,error:{code:-0,message:'native-message',data:errorData}};
const error=output.classifyPublicTestResponse(frame,new Set([7]));error.error.data.added=88;
aliases.push({kind:'error',same:error.error.data===errorData,input:pack(errorData)});
process.stdout.write(JSON.stringify({notifications,responses,raw,aliases}));
