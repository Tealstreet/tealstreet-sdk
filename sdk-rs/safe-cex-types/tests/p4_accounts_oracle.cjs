const path=require('node:path');
const {root,load}=require('../scripts/accounts-source.cjs');
const base=path.join(root,'packages/safe-cex/src');
const build=load(path.join(base,'accounts/buildExchangeAccountConfig.ts'));
const normalize=load(path.join(base,'accounts/normalizeExchangeAccountConfig.ts'));
const imports=load(path.join(base,'accounts/parseImportAccountData.ts'));
const metadata=load(path.join(base,'exchangeMetadata.ts'));
const rows=[];
function add(op,input){let expected;try {const value=({build:build.buildExchangeAccountConfig,normalize:normalize.normalizeExchangeAccountConfig,schema:build.getExchangeAccountCredentialSchema,parse:imports.parseImportAccountData,valid:imports.isValidExchangeAccount,names:build.getSupportedExchangeAccountNames,schemas:build.getSupportedExchangeAccountCredentialSchemas}[op])(input);expected={ok:value===undefined?null:value};}catch(e){expected={error:{name:e.name,message:e.message,missingFields:e.missingFields||[]}};}rows.push({op,input:input===undefined?null:input,expected});}
add('names');add('schemas');
const full={name:' main ',apiKey:' key ',apiSecret:' secret ',passphrase:' pass ',applicationId:' id ',apiKeyIndex:' 1 ',omniKeySeed:' seed ',paradexAddress:' address ',subkeyAddress:' subkey ',grvtSignerAddress:' signer ',grvtTradingAccountId:' trading '};
for(const exchange of [...Object.keys(metadata.exchangeMetadata),'constructor','__proto__','UNKNOWN',' BYBIT ']){
add('schema',exchange);
for(const patch of [{},{apiKey:null},{apiSecret:''},{name:'\ufeff '},{tealstreetProxy:true},{testnet:true,tealstreetProxy:true},{privateKey:' pk ',walletAddress:' wallet ',apiKey:'',apiSecret:'',secret:'legacy'},{bybitSiteId:'BRA_BTL'},{bybitSiteId:' BRA_BTL '},{proxyRegion:' JA ',tealstreetProxy:1},{name:0},{unknown:{x:9007199254740993}}]){const input={exchange,...full,...patch};add('build',input);add('valid',input);add('parse',JSON.stringify(input));}
for(const testnet of [false,true,0,1,'',[],{}])for(const tealstreetProxy of [false,true,null,'',[],{}])for(const proxyRegion of [null,'','JA',0,[],{}])add('normalize',{exchange,testnet,tealstreetProxy,proxyRegion});
}
for(const input of [null,0,false,'str',[],{}, {exchange:{}},{exchange:{toString:null}}, {exchange:['wat']},{exchange:'bybit',name:'\u0085'}, {exchange:'bybit',name:'x',apiKey:'k',apiSecret:'s','2':'two','1':'one'}]){add('build',input);add('valid',input);add('parse',JSON.stringify(input));}
for(const raw of ['', '{', 'null','[]','[null]','[{}]','123','true','"abc"', JSON.stringify([{exchange:'bybit',...full},{name:'bad'}]),JSON.stringify({exchange:'bybit',name:'x',apiKey:'k',secret:'s',apiSecret:false})])add('parse',raw);
// Pin the two disclosed import divergences without transporting lone surrogates to Rust.
for (const unknown of ['\ud800',JSON.parse('['.repeat(130)+'0'+']'.repeat(130))]) {
 if (!imports.parseImportAccountData(JSON.stringify({exchange:'bybit',name:'x',apiKey:'k',apiSecret:'s',unknown})).isValid) throw Error('source importer representation boundary changed');
}
process.stdout.write(JSON.stringify(rows));
