const load=require('./load-module.cjs');
const path=require('node:path');
const unavailable=new Proxy(function(){throw Error('Unselected dependency invoked');},{get(_,key){if(key==='__esModule')return false;return ()=>{throw Error('Unselected dependency invoked: '+String(key));};}});
const compilerPath=process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(__dirname,'../../../target/asterdex-oracle-deps/node_modules/typescript');
const compilerRoot=path.resolve(path.dirname(require.resolve(compilerPath)),'..');
const ethersName=process.env.ASTERDEX_API_PACKAGE_CONTROL==='missing-ethers'?'asterdex-missing-ethers-control':'ethers';
const ethers=require(path.join(path.dirname(compilerRoot),ethersName));
const retry=load('packages/safe-cex/src/utils/retryReadsOnly.ts',[],{'axios-retry':unavailable});
let clockNow=0;
const api=load('packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts',['buildV3Nonce','isAsterdexRead','buildQueryString','buildV3SigningPayload','stripQueryString','isPublicEndpoint','isAsterDEXCodeSignedEndpoint','shouldSuppressUnknownOrderError','getAsterDEXErrorDetail','generateV1Signature','generateAsterDEXCodeMessageSignature'],{
 crypto:require('node:crypto'),axios:unavailable,'axios-rate-limit':unavailable,'axios-retry':unavailable,ethers,
 '../../createApiWithSecondaryKey':unavailable,'../../safeCexAxiosCreate':unavailable,'../../types':unavailable,'../../utils/signatureRejection':load('packages/safe-cex/src/utils/signatureRejection.ts',[],{}),'../../utils/httpFailure':unavailable,'../../utils/retryReadsOnly':retry,'./asterdex.types':require('./source.cjs').values,'./asterdex.virtualClock':{asterDEXVirtualClock:{getCurrentTimeMs:()=>clockNow}},
},source=>{
 if(process.env.ASTERDEX_API_SIGNATURE_CONTROL==='wrong-chain'){
  const before='chainId: getAsterDEXCodeChainId(testnet)';
  if(!source.includes(before))throw Error('AsterDEX signature source control target missing');
  return source.replace(before,'chainId: getAsterDEXCodeChainId(!testnet)');
 }
 return source;
});
api.setOracleClock=value=>{clockNow=value};
module.exports=api;
