import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';

const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
if(!owner)throw Error('missing MexcExchange declaration');
const member=owner.members.find(node=>ts.isPropertyDeclaration(node)&&node.name?.getText(ast)==='fetchLeverageForSymbol');
const metadataMember=owner.members.find(node=>ts.isPropertyDeclaration(node)&&node.name?.getText(ast)==='updateSymbolMetadata');
const updateFetchedMember=owner.members.find(node=>ts.isPropertyDeclaration(node)&&node.name?.getText(ast)==='updateFetchedLeverage');
if(!member)throw Error('missing MexcExchange.fetchLeverageForSymbol source');
if(!metadataMember)throw Error('missing MexcExchange.updateSymbolMetadata source');
if(!updateFetchedMember)throw Error('missing MexcExchange.updateFetchedLeverage source');
let memberSource=member.getText(ast);
if(process.env.MEXC_LEVERAGE_TS_CONTROL==='buy-leverage'){
 const before='const buyLeverage = toNumber(longRow?.leverage, config.buyLeverage);';
 if(!memberSource.includes(before))throw Error('missing TypeScript mutation anchor');
 memberSource=memberSource.replace(before,'const buyLeverage = 999;');
}
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const extractRowsSource=source.slice(source.indexOf('const extractData ='),source.indexOf('\n\ntype MexcSymbolConfig'));
const code=`${extractRowsSource}\nclass Holder { ${memberSource}\n${updateFetchedMember.getText(ast)} }\nHolder`;
const compiled=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const Holder=vm.runInNewContext(compiled,{toNumber:utils.toNumber,normalizeMarginType:utils.normalizeMarginType,ENDPOINTS:types.ENDPOINTS,REVERSE_MARGIN_TYPE:types.REVERSE_MARGIN_TYPE,LogSeverity:{Warning:'warning'},Promise,Map,Number,console});

const config={marginType:'cross',buyLeverage:21,sellLeverage:7,positionMode:'hedged'};
async function run({publicDataOnly=false,hasMarket=true,requestError=false}={}){
 const market=hasMarket?{id:'BTC_USDT',symbol:'BTCUSDT',limits:{amount:{min:0,max:10},leverage:{min:1,max:20}}}:undefined;
 const store={marketsBySymbol:market?{BTCUSDT:market}:{},fetchedLeverage:{},updateMarket(_query,patch){Object.assign(market,patch);},update(patch){Object.assign(this,patch);}};
 const instance=new Holder();
 Object.assign(instance,{isPublicDataOnly:publicDataOnly,store,symbolSettings:new Map([['BTCUSDT',{...config}]]),xhr:{async get(){if(requestError)throw Error('request failed');return {data:[{positionType:1,leverage:'31',maxLeverageView:'80',openType:2},{positionType:2,leverage:'12',maxLeverage:'60',openType:1}]};}},log(){}});
 const fetched=await instance.fetchLeverageForSymbol('BTCUSDT');
 return {fetched,market:market??null,store:store.fetchedLeverage,config:instance.symbolSettings.get('BTCUSDT')};
}
const results={success:await run(),publicFallback:await run({publicDataOnly:true}),missingMarket:await run({hasMarket:false}),requestFailure:await run({requestError:true})};
let metadataSource=metadataMember.getText(ast);
if(process.env.MEXC_LEVERAGE_TS_CONTROL==='metadata'){
 const before='await this.fetchLeverageForSymbol(symbol);';
 if(!metadataSource.includes(before))throw Error('missing metadata mutation anchor');
 metadataSource=metadataSource.replace(before,'await Promise.resolve();');
}
const metadataCode=ts.transpileModule(`class MetadataHolder { ${metadataSource} }\nMetadataHolder`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const MetadataHolder=vm.runInNewContext(metadataCode,{Promise});
const metadataCalls=[];
const metadata=new MetadataHolder();
metadata.fetchLeverageForSymbol=async symbol=>metadataCalls.push(symbol);
await metadata.updateSymbolMetadata('BTCUSDT');
results.metadataCalls=metadataCalls;
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const actualStore=new DefaultStore();
actualStore.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:1,maxLeverage:20})]});
const subscriberCalls=[];
actualStore.subscribe(()=>{subscriberCalls.push('first');throw Error('subscriber failed');});
actualStore.subscribe(()=>subscriberCalls.push('second'));
const notifying=new Holder();
Object.assign(notifying,{isPublicDataOnly:false,store:actualStore,symbolSettings:new Map([['BTCUSDT',{...config}]]),xhr:{async get(){return {data:[{positionType:1,leverage:'31',maxLeverageView:'80',openType:2},{positionType:2,leverage:'12',maxLeverage:'60',openType:1}]};}},log(){}});
try{await notifying.fetchLeverageForSymbol('BTCUSDT');results.subscriberThrow={rejected:false};}
catch(error){results.subscriberThrow={rejected:true,message:error.message,marketMax:actualStore.marketsBySymbol.BTCUSDT.limits.leverage.max,fetchedBuy:actualStore.fetchedLeverage.BTCUSDT.buyLeverage,subscriberCalls};}
// Supplement: stored source settings must short-circuit deriveSymbolConfig even
// while the Store still holds a position from the previous configuration.
results.storedConfig={};
for(const name of ['emptyRows','untypedRows','missingMarket']){
 const settings={buyLeverage:50,sellLeverage:50,marginType:'cross',positionMode:'hedged'};
 const market={id:'BTC_USDT',symbol:'BTCUSDT',limits:{amount:{min:0,max:10},leverage:{min:1,max:125}}};
 const stale={id:'stale',symbol:'BTCUSDT',side:'long',contracts:2,leverage:20,marginType:'isolated',positionMode:'oneway',info:{openType:1}};
 const store={marketsBySymbol:name==='missingMarket'?{}:{BTCUSDT:market},positions:[stale],fetchedLeverage:{},updateMarket(){throw Error('unexpected market update');},update(patch){Object.assign(this,patch);}};
 let requests=0;
 const instance=new Holder();
 Object.assign(instance,{isPublicDataOnly:false,store,symbolSettings:new Map([['BTCUSDT',settings]]),deriveSymbolConfig(){throw Error('stored settings must skip derivation');},xhr:{async get(){requests++;return {data:name==='untypedRows'?[{}]:[]};}},log(){}});
 const fetched=await instance.fetchLeverageForSymbol('BTCUSDT');
 results.storedConfig[name]={fetched,stored:store.fetchedLeverage.BTCUSDT,config:instance.symbolSettings.get('BTCUSDT'),requests};
}
process.stdout.write(JSON.stringify(results));
