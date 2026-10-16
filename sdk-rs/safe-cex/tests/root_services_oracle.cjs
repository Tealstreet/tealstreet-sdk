const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const metadata=load(path.join(root,'packages/safe-cex/src/exchangeMetadata.ts'));
const routing=load(path.join(root,'packages/safe-cex/src/transport/tealstreet/transportResolution.ts'));
const site=load(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.site.ts'));
function evaluate(relative,extra,params,values,names) {
 const file=path.join(root,relative),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const statements=ast.statements.filter(n=>!ts.isImportDeclaration(n)&&!ts.isExportDeclaration(n)&&(!names|| (ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast)))) || (ts.isFunctionDeclaration(n)&&names.includes(n.name.text))));
 if(names) assert.equal(statements.length,names.length,'FACTORY independently expected selected declaration count');
 const code=ts.transpileModule(statements.map(n=>n.getText(ast)).join('\n')+extra,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const exportsValue={};new Function('exports',...params,code)(exportsValue,...values);return exportsValue;
}
const provider={publicCorsProxy:()=> 'https://cors',serverProxyUrl:()=> 'https://server'};
const names=['bybit','lighter','grvt','kucoinspot','paradex','binance'];
const accounts=names.map(exchange=>({exchange,name:'fixture',apiKey:'primary',apiSecret:'secret',secondary_apiKey:'read',secondary_apiSecret:'readsecret',secondary_passphrase:'readpass',passphrase:'pass',testnet:true,bybitSiteId:'BRA_BTL',enableSpotMarkets:true,applicationId:'application',apiKeyIndex:'index',vault:'vault',omniKeySeed:'seed',paradexAddress:'paradex',subkeyAddress:'subkey',grvtSignerAddress:'signer',grvtTradingAccountId:'trading',subaccount:'sub',orderlyExchangeId:'orderly',tealstreetWsRoute:true}));
const env={NEXT_PUBLIC_KUCOIN_BROKER_PARTNER:'partner',NEXT_PUBLIC_KUCOIN_BROKER_KEY:'brokerkey',NEXT_PUBLIC_KUCOIN_BROKER_NAME:'brokername',NEXT_PUBLIC_KUCOIN_BROKER_VERIFY:'yes'};
const account=evaluate('packages/safe-cex/src/createExchangeFromAccount.ts','',['getExchangeMetadata','resolveAccountTransport','normalizeBybitSiteId','getTransport','createExchange','process'],[metadata.getExchangeMetadata,routing.resolveAccountTransport,site.normalizeBybitSiteId,()=>provider,async(name,options)=>({name,options}),{env}]);
const overrides={key:'override',tealstreetProxy:false,extra:{tickInterval:25,kucoinBroker:undefined}};
const options=accounts.map(a=>account.accountToExchangeOptions(a,overrides));
assert.equal(options[0].key,'override');assert.equal(options[0].enableSpotMarkets,undefined);assert.equal(options[1].applicationId,'application');assert.equal(options[1].apiKeyIndex,'index');assert.equal(options[2].grvtSignerAddress,'signer');assert.equal(options[2].grvtTradingAccountId,'trading');assert.equal(options[3].extra.kucoinBroker,undefined);assert.equal(options[3].extra.tickInterval,25);assert.equal(options[4].paradexAddress,'paradex');assert.equal(options[4].subkeyAddress,'subkey');
let loads=0,clockStarts=0;class Fixture {constructor(name,options,store){Object.assign(this,{name,options,store})}}class DefaultStore{}
const registry={bybit:async()=>{loads++;return Fixture},binance:async()=>Fixture};
const factory=evaluate('packages/safe-cex/src/index.ts','',['getExchangeMetadata','getTransport','exchangeRegistry','getRegisteredExchangeNames','virtualClock','DefaultStore'],[metadata.getExchangeMetadata,()=>provider,registry,()=>Object.keys(registry),{start(){clockStarts++}},DefaultStore],['exchangeCache','loadExchange','createExchange','buildPublicExchangeOptions','preloadExchange','preloadExchanges','isExchangeLoaded','getAvailableExchanges','createPublicExchange']);
(async()=>{
 assert.equal(factory.isExchangeLoaded('bybit'),false);await factory.preloadExchange('bybit');await factory.preloadExchanges(['bybit','binance']);assert.equal(loads,1);assert.deepEqual(factory.getAvailableExchanges(),['bybit','binance']);
 const supplied={identity:true};const instance=await factory.createExchange('bybit',options[0],supplied);assert.equal(instance.store,supplied);assert.equal(clockStarts,1);assert.equal(instance.options,options[0]);
 const pub=factory.buildPublicExchangeOptions('bybit',{overrideProxyUrl:'explicit',tealstreetProxy:true});assert.equal(pub.overrideProxyUrl,'explicit');assert.equal(pub.publicDataOnly,true);assert.equal(pub.key,'');
 const disabled=factory.buildPublicExchangeOptions('bybit',{forceAllProxiesOff:true,overrideProxyUrl:'explicit',tealstreetProxy:true,tealstreetTunnel:true,tealstreetWsRoute:true});assert.equal(disabled.overrideProxyUrl,undefined);assert.equal(disabled.tealstreetProxy,false);assert.equal(disabled.tealstreetTunnel,false);assert.equal(disabled.tealstreetWsRoute,false);
 await assert.rejects(factory.preloadExchange('unknown'),/Unknown exchange: unknown/);
 const publicInstance=await factory.createPublicExchange('binance');assert.equal(publicInstance.options.publicDataOnly,true);assert.ok(publicInstance.store instanceof DefaultStore);
 console.log(JSON.stringify({accounts,options,pub,disabled,stoppedAfterAccounts:6,stoppedAfterFactoryCases:8}));
})().catch(e=>{console.error(e);process.exitCode=1});
