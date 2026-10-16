const api=require('./api-source.cjs');
const result=fn=>{try{return {value:fn()}}catch(error){return {error:{name:error.name,message:error.message}}}};
const chains=[false,true].map(input=>({input,id:api.getAsterDEXCodeChainId(input),chain:api.getAsterDEXAsterChain(input)}));
const payloads=[{}, {a:0,b:false,c:null,d:'',e:[],f:[1,null,'x'],g:{x:1}},JSON.parse('{"10":"ten","2":"two","01":"one","z":"Z","a":"A","__proto__":"drop","constructor":"keep"}'),{'\ue000':'bmp','😀':'astral','a&b':'x=y','bom':'\ufeff'}, {x:{toString:null}}, {a:1e-7,b:1e21,c:-0,d:1e20}];
const queries=payloads.map(original=>{const entries=Object.entries(original).reverse();const input=Object.fromEntries(entries);return {input,entries,code:result(()=>api.buildAsterDEXCodeQueryString(input)),query:result(()=>api.buildQueryString(input)),signing:result(()=>api.buildV3SigningPayload(input)),signingKeys:result(()=>Object.keys(api.buildV3SigningPayload(input)))};});
const paths=[];for(const url of [undefined,'','/fapi/v1/ticker/24hr','https://host/fapi/v3/order?foo','/fapi/v3/builder?x','/fapi/v3/agent','/garbage/fapi/v1/timeSuffix','/x?path=/fapi/v1/time','/fapi/v4/time','/fapi/v3/listenKey','/fapi/v1/order'])for(const method of [undefined,'','get','GET','head','post','delete','options']){
 const config={...(url===undefined?{}:{url}),...(method===undefined?{}:{method})};paths.push({config,public:api.isPublicEndpoint(url),signed:api.isAsterDEXCodeSignedEndpoint(url,method),read:api.isAsterdexRead(config),stripped:url===undefined?null:api.stripQueryString(url)});
}
const errors=[];for(const message of [undefined,null,'',false,0,[],'Unknown ORDER sent','cannot found builder config','ordinary',{error:1},{toString:null}])for(const code of [undefined,-2011,'-2011',-5025,'-5025',{toString:null}])for(const suppress of [false,true,[],0]){
 const config={__suppressAsterUnknownOrderError:suppress};errors.push({...(message===undefined?{}:{message}),...(code===undefined?{}:{code}),config,suppressed:result(()=>api.shouldSuppressUnknownOrderError(config,code,message)),detail:result(()=>api.getAsterDEXErrorDetail(message,code))});
}
const signatures=['','abc','x=1&msg=☃','x=1e-7'].map(query=>({query,secret:'test-only-secret',signature:api.generateV1Signature(query,'test-only-secret')}));
const messageSignatures=[];
for(const query of ['', 'symbol=ETHUSDT&time=1780000000000', 'q=☃&space=\ufeff'])for(const testnet of [undefined,false,true,null]){
 const key='0x'+'1'.padStart(64,'0');
 messageSignatures.push({query,...(testnet===undefined?{}:{testnet}),key,signature:result(()=>api.generateAsterDEXCodeMessageSignature(query,key,testnet))});
}
for(const key of ['0x'+'0'.repeat(64),'not-a-private-key','0x1','g'.repeat(64),'f'.repeat(64),'0X'+'1'.padStart(64,'0'),' '+ '1'.padStart(64,'0'),'','0x0x'+'1'.padStart(64,'0')]){
 messageSignatures.push({query:'signed-error-shape',testnet:false,key,signature:result(()=>api.generateAsterDEXCodeMessageSignature('signed-error-shape',key,false))});
}
const unsigned=[{symbol:'BTC',signature:'s',timestamp:1,recvWindow:2,user:'u',signer:'v',nonce:'n',keep:0},{2:'b',1:'a',z:1,nonce:null},['x','y'],null,'text',0,{},{Signature:'case'}].map(input=>({input,output:api.withoutSigningFields(input),keys:Object.keys(api.withoutSigningFields(input))}));
process.stdout.write(JSON.stringify({chains,queries,paths,errors,signatures,messageSignatures,unsigned}));
