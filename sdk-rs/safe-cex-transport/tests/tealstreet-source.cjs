const fs=require('node:fs'), assert=require('node:assert/strict');
const {root,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
function load(file,imports={},extra='',globals={}) {
 const capture={exports:{}};
 const code=ts.transpileModule(fs.readFileSync(root+'/'+file,'utf8')+extra,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 const keys=Object.keys(globals);
 new Function('module','exports','require',...keys,code)(capture,capture.exports,name=>{assert.ok(Object.hasOwn(imports,name),'unexpected import '+name);return imports[name];},...keys.map(key=>globals[key]));
 return capture.exports;
}
const p='packages/safe-cex/src/transport/tealstreet/';
const pools=load(p+'transportResolution.ts');
const egress=load('packages/http-over-websocket/src/egressRegion.ts');
const endpoints=load('packages/http-over-websocket/src/tunnelEndpoints.ts','');
const metadata=ts.createSourceFile('metadata.ts',fs.readFileSync(root+'/packages/safe-cex/src/exchangeMetadata.ts','utf8'),ts.ScriptTarget.Latest,true);
let names=[];
function visit(node){if(ts.isVariableDeclaration(node)&&node.name.getText(metadata)==='exchangeMetadata'){let value=node.initializer;while(ts.isAsExpression(value)||ts.isSatisfiesExpression(value))value=value.expression;names=value.properties.map(property=>property.name.text);}ts.forEachChild(node,visit);}
visit(metadata);assert.equal(names.length,49,'independent metadata-key floor');
const table=load(p+'routing/routingTable.ts',{'@tealstreet/http-over-websocket/src/egressRegion':egress,'../../../exchangeMetadata':{exchangeMetadata:Object.fromEntries(names.map(name=>[name,{}]))}});
const routing=load(p+'routing/resolveExchangeRouting.ts',{'@tealstreet/http-over-websocket/src/tunnelEndpoints':endpoints,'../transportResolution':pools,'./routingTable':table});
const diag=load('packages/safe-cex/src/utils/venueRestDiagnostics.ts',{'./unknownSendOutcome':load('packages/safe-cex/src/utils/unknownSendOutcome.ts')});
const adapters={default:function Default(){},bigInt:function BigInt(){}};
let token='token',node=true,events=[];
const provider=load(p+'tealstreetTransport.ts',{
 '@tealstreet/http-over-websocket/src/tunnelEndpoints':endpoints,
 '../../accessToken':{getProxyToken:()=>{events.push('token');return Promise.resolve(token);}},
 '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter':{axiosHttpOverWebsocketAdapter:adapters.default,bigIntAxiosHttpOverWebsocketAdapter:adapters.bigInt,createAxiosHttpOverWebsocketAdapter:opts=>({region:opts.region})},
 './routing/resolveExchangeRouting':routing,'../../utils/runtime':{isNodeOrBunRuntime:()=>node},
 './transportResolution':pools,'../../utils/venueRestDiagnostics':{markRestProxyHeader:(request,shape)=>{events.push('mark:'+shape);return diag.markRestProxyHeader(request,shape);}},
},'\nexport {isTealstreetWebSocketUrl};');
const relay=load(p+'wsRelay.ts');
const directProvider=load('packages/safe-cex/src/transport/direct.ts').directTransport;
(async()=>{
 const transport=provider.tealstreetTransport;
 const cases=require('./fixtures/tealstreet-cases.json');assert.equal(cases.length,22,'independent provider-case floor');
 const observations=cases.map(input=>{
  const context={exchange:input.exchange,options:input.options,config:input.config};
  const rest=transport.rest(context);
  assert.equal(typeof rest.authorize,'function');
  return {...input,expected:{baseURL:rest.baseURL,proxied:rest.proxied,prefix:transport.proxyPrefix(context),requireProxy:transport.requireProxy(input.options).tealstreetProxy,privateUrl:transport.privateProxy(input.exchange).url,serverUrl:transport.serverProxyUrl(input.exchange),routing:transport.wsRouting(input.exchange,input.options)}};
 });
 assert.equal(observations[4].expected.proxied,false,'source override does not change proxied');
 assert.equal(observations[8].expected.proxied,true,'source GRVT tunnel exception');
 assert.equal(observations[10].expected.proxied,true,'source Blofin tunnel exception');
 assert.equal(observations[12].expected.prefix,pools.AWS_PUBLIC_CORS_PROXY,'source Binance public precedence');
 assert.equal(transport.restAdapter({variant:'default'}),adapters.default);assert.equal(transport.restAdapter({variant:'bigInt'}),adapters.bigInt);assert.equal(transport.restAdapter({variant:{region:'JA'}}).region,'JA');
 const auth=[];
 for(const exchange of ['bybit','grvt','mexc'])for(const value of ['', 'token'])for(const native of [false,true]){
  token=value;node=native;events=[];const request={headers:{Authorization:'venue-auth'},params:{'2':-0,'1':1}};const params=request.params;
  const promise=transport.rest({exchange,options:{},config:{}}).authorize(request);assert.deepEqual(events,['token'],'source eager token call');await promise;
  assert.equal(request.params,params,'source params identity retained');assert.equal(Object.is(request.params['2'],-0),true);
  assert.equal(request.headers.Authorization,exchange==='bybit'&&value?'Bearer token':'venue-auth');
  assert.equal(request.headers['Tealstreet-Authorization'],exchange!=='bybit'&&value?'Bearer token':undefined);
  assert.equal(request.safeCexRestDiagProxyHeader,value?'present':'empty');assert.equal(request.headers.Origin,native?'https://trade.tealstreet.io':undefined);
  const privateHeaders=await transport.privateProxy(exchange).resolveHeaders();assert.equal(Object.keys(privateHeaders).length,value?1:0);
  auth.push({exchange,token:value,node:native,headers:request.headers,privateHeaders,events:events.slice(0,2)});
 }
 const resolutions=[];
 for(const proxy of [undefined,false,true])for(const tunnel of [undefined,false,true])for(const cors of [false,true])for(const enforce of [undefined,false,true])for(const support of [undefined,false,true]){
  const account={tealstreetProxy:proxy,tealstreetTunnel:tunnel,proxyRegion:''};const meta={requiresCors:cors,enforceWsTunnel:enforce,supportsTunnel:support,privateProxyRegions:{JA:[],SG:[]}};
  const result=pools.resolveAccountTransport(account,meta);assert.equal(Object.hasOwn(result,'tealstreetProxy'),true);assert.equal(Object.hasOwn(result,'tealstreetTunnel'),true);
  resolutions.push({account,meta,result,direct:pools.isDirectTransport(result)});
 }
 assert.equal(resolutions.length,162,'independent preference-case floor (3*3*2*3*3)');assert.equal(pools.defaultProxyRegion(undefined),'SG');assert.equal(pools.defaultProxyRegion({privateProxyRegions:{'':''}}),'SG');assert.equal(pools.defaultProxyRegion({privateProxyRegions:{JA:[],2:[],1:[]}}),'1');
 const urls=['wss://tealstreet.io','wss://sub.tealstreet.io','wss://TEALSTREET.IO','wss://tealstreet.io.evil','bad url','https://venue.test/?ts-relay=','wss://tealstreet.io/?ts-relay=x'];
 const connections=urls.map(url=>({url,tealstreet:provider.isTealstreetWebSocketUrl(url),connection:transport.wsConnection(url)}));assert.equal(connections[5].connection.kind,'relay');assert.equal(connections[6].connection.kind,'relay');
 assert.equal(relay.isWsRelayEnabled(),false);relay.setWsRelayEnabled(true);assert.equal(relay.isWsRelayEnabled(),true);relay.setWsRelayEnabled(1);assert.equal(relay.isWsRelayEnabled(),false);
 let clock=1000, origin,configured,reply={unixMs:2000},calls=[];
 const dayjs=dependencies('dayjs'),utc=dependencies('dayjs/plugin/utc');
 const dateBinding={extend:plugin=>dayjs.extend(plugin),utc:value=>{calls.push('parse');clock+=100;return dayjs.utc(value);}};
 const time=load(p+'tealstreetTimeSource.ts',{'axios':{get:(url,options)=>{calls.push([url,options.timeout]);return Promise.resolve({data:reply});}},'dayjs':dateBinding,'dayjs/plugin/utc':utc},'',{Date:{now:()=>{calls.push('clock');return clock;}},self:{get location(){return{origin};}},process:{env:{get NEXT_PUBLIC_TIME_API_ORIGIN(){return configured;}}}});
 const endpointCases=[undefined,'',' / ','https://time.example///',' https://time.example/api/time/ ','\uFEFFhttps://time.example\uFEFF'];
 const times=endpointCases.map(value=>{configured=value;origin='https://app';return{configured:value,endpoint:time.getTealstreetTimeEndpoint()};});
 configured=undefined;origin=undefined;calls=[];clock=1000;let pending=time.tealstreetTimeSource();assert.deepEqual(calls,['clock',['/api/time',5000]],'source eager time request');clock=1007;assert.equal(await pending,2003);
 reply={time:'1970-01-01T00:00:02.000Z'};clock=1000;calls=[];pending=time.tealstreetTimeSource();clock=1007;assert.equal(await pending,2003);assert.deepEqual(calls,['clock',['/api/time',5000],'clock','parse'],'source captures received time before date parsing');
 reply={unixMs:2000};clock=1000;pending=time.tealstreetTimeSource();clock=997;assert.equal(await pending,1998,'source negative RTT floors');
 reply={unixMs:NaN};assert.equal(await time.tealstreetTimeSource(),undefined);reply={time:'invalid'};assert.equal(await time.tealstreetTimeSource(),undefined);
 const routes=[];
 for(const exchange of names)for(const tealstreetProxy of [false,true])for(const tealstreetWsRoute of [false,true])for(const proxyRegion of [undefined,'JA','SG'])for(const bybitSiteId of [undefined,'BRA_BTL']){
  const options={tealstreetProxy,tealstreetWsRoute,proxyRegion,bybitSiteId};
  routes.push({exchange,options,expected:routing.resolveExchangeRouting(exchange,options)});
 }
 assert.equal(routes.length,1176,'independent routing floor (49*2*2*3*2)');
 const direct={name:directProvider.name,...directProvider.rest(cases[0])};
 process.stdout.write(JSON.stringify({pools,cases:observations,auth,resolutions,connections,times,routes,direct}));
})().catch(error=>{console.error(error);process.exitCode=1;});
