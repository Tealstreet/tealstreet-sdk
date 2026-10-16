const {values:v}=require('./source.cjs');
const guards=[];
const samples=[undefined,null,false,0,[],{},'', 'EXPIRE_TAKER','EXPIRE_MAKER','EXPIRE_BOTH','expire_taker',' EXPIRE_TAKER','EXPIRE_TAKER\ufeff',v.ASTERDEX_MAIN_WALLET_REQUIRED_HINT,'AUTH.USERADDRESS','V3 KEYS REQUIRE','prefix '+v.ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE];
for(const input of samples)guards.push({input:input??null,stp:v.isAsterDEXStpMode(input),wallet:v.isAsterDEXMainWalletError(input)});
const strings=[undefined,'',' ', '\ufeff','\u0085','\ufeff x \ufeff','\u0085x\u0085','0x'+'a'.repeat(40),'0X'+'a'.repeat(40),'a'.repeat(64),'0x'+'A'.repeat(64),'0x'+'a'.repeat(63),'0x'+'g'.repeat(64),'0x'+'a'.repeat(40)+'\n'];
const checks=strings.map(input=>({input:input??null,trim:v.trimNonEmpty(input)??null,address:v.isAsterDEXAddress(input),key:v.isAsterDEXV3PrivateKey(input)}));
const address='0x'+'1'.repeat(40),signer='0x'+'2'.repeat(40),key='a'.repeat(64);
const options=[];
for(const base of [{key:'',secret:''},{key:signer,secret:key,applicationId:address},{key:signer,secret:key,applicationId:address,auth:{userAddress:' \ufeff',signerAddress:'\ufeff'+signer+' ',privateKey:' '+key+'\ufeff'}},{key:signer,secret:key,applicationId:address,auth:{userAddress:'invalid',signerAddress:'invalid',privateKey:'invalid'}},{key:'x',secret:'x',applicationId:'x',auth:{userAddress:address,signerAddress:signer,privateKey:key}}])for(const extra of [undefined,{asterdexApiVersion:'v1'},{asterdexApiVersion:'v3'}]){
 const input={...base,...(extra?{extra}:{})};options.push({input,context:v.resolveAsterDEXV3AuthContext(input),has:v.hasAsterDEXV3AuthContext(input),version:v.resolveAsterDEXApiVersion(input)});
}
const paths=[];for(const endpoint of ['','/','///order',' /order','/order?x=1','\ufeff/order'])for(const version of ['v1','v3','v4'])paths.push({endpoint,version,output:v.getAsterDEXFapiPath(endpoint,version)});
const trade=(id,time,realizedPnl,positionSide='BOTH',symbol='BTCUSDT')=>({symbol,id,orderId:1,price:'1',qty:'1',side:'BUY',time,...(realizedPnl===undefined?{}:{realizedPnl}),positionSide});
const absentSide=trade(1,1,'9');delete absentSide.positionSide;
const pnl=[[],[absentSide,trade(2,2,'12'),trade(1,1,'50','BOTH','ETHUSDT'),trade(2,2,'52','BOTH','ETHUSDT')],[trade(1,1,'100'),trade(2,2,'105'),trade(3,3,'99')],[trade(2,2,'10'),trade(1,1,'5'),trade(3,3,'9','long'),trade(4,4,'12','LONG'),trade(5,5,'7',''),trade(6,6,'15','SHORT')],[trade(1,1,undefined),trade(2,2,'bad'),trade(3,3,'0x10'),trade(4,4,'12abc'),trade(5,5,'Infinity'),trade(6,6,'1e309'),trade(7,7,'\ufeff12.5')],[trade(2,1,'4'),trade(1,1,'3'),trade(1,1,'7')],[trade(1,1,'-1e308'),trade(2,2,'1e308')]].map(input=>({input,output:v.normalizeAsterDEXRealizedPnlDeltas(input),keys:input.map(v.getAsterDEXRealizedPnlKey)}));
process.stdout.write(JSON.stringify({guards,checks,options,paths,pnl}));
