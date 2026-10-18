import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const parse = path => ts.createSourceFile(path,readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
const context=vm.createContext({nanoid:()=> 'fixture'});
const evaluate = text => vm.runInContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText, context);
const tables=parse('exchanges/bybit/bybit.types.ts');
for(const name of ['BASE_WS_URL','DEMO_WS_URL']) {
 const declaration=tables.statements.flatMap(s=>ts.isVariableStatement(s)?[...s.declarationList.declarations]:[]).find(d=>d.name.getText(tables)===name);
 assert.ok(declaration,name);evaluate(`const ${name}=${declaration.initializer.getText(tables)};`);
}
const fallback=parse('utils/wsDirectFallback.ts');
const klass=fallback.statements.find(ts.isClassDeclaration);evaluate(klass.getText(fallback).replace(/^export /,''));
const site=parse('exchanges/bybit/bybit.site.ts');
// Evaluate source declarations, excluding imports and export-only statements.
for(const declaration of site.statements) {
 if(ts.isFunctionDeclaration(declaration)||ts.isVariableStatement(declaration)) evaluate(declaration.getText(site).replace(/^export /,''));
}
for(const [kind,name,file] of [['Public','BybitPublicWebsocket','bybit.ws-public.ts'],['Private','BybitPrivateWebsocket','bybit.ws-private.ts']]) {
 const source=parse(`exchanges/bybit/${file}`), cls=source.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);
 const member=cls.members.find(m=>m.name?.getText(source)==='connectAndSubscribe');assert.ok(member);
 evaluate(`class ${kind} {${member.getText(source)}}; globalThis.${kind}=${kind};`);
}
const cases=[];
const routes=[{}, {tunnelled:true,host:'wss://tunnel'}, {tunnelled:true,host:''}, {fallback:'wss://fallback'}, {relay:'wss://relay/ws?ts-relay=wss%3A%2F%2Fupstream',fallback:'wss://fallback'}];
for(const privateSocket of [false,true]) for(const category of privateSocket?['linear']:['linear','inverse'])
for(const demo of [false,true]) for(const testnet of [false,true]) for(const siteId of [undefined,'BRA_BTL'])
for(const routeInput of routes) for(const variation of ['normal','override','emptyOverride','disposed','signer','relayDisabled']) {
 const route={tunnelled:false,...routeInput}, relayEnabled=variation!=='relayDisabled', disposed=variation==='disposed', signer=variation==='signer';
 const override=variation==='override'?'wss://custom?existing=1':variation==='emptyOverride'?'':null;
 const options={key:'',secret:'',testnet,...(siteId?{bybitSiteId:siteId}:{}),...(signer?{payloadSigner:{}}:{})};
 const calls=[],events=[];const venue=privateSocket?new context.Private():new context.Public();
 context.resolveTransportRouting=()=>({wsPublic:route,wsPrivate:route});
 context.isWsRelayEnabled=()=>relayEnabled;
 context.getTransport=()=>({wsConnection:url=>url.includes('relay')?{kind:'relay'}:undefined});
 venue.parent={name:'bybit',options,isDemo:demo,emitter:{emit:(...args)=>events.push(args)}};
 if(override!==null){venue.parent.getBybitPublicWsUrl=()=>override;venue.parent.getBybitPrivateWsUrl=()=>override;}
 venue.category=category;venue.isDisposed=disposed;venue.directFallback=vm.runInContext('new WsDirectFallback()',context);
 venue.createWebsocket=(url,protocols,options)=>{assert.equal(protocols,undefined);calls.push({url,headers:options?.headers??{}});return {addEventListener(){}};};
 for(let i=0;i<6;i++) {
  venue.connectAndSubscribe();
  assert.equal(calls.length, disposed || (privateSocket && signer) ? 0 : i+1,
   'maintained connectAndSubscribe creates its socket synchronously before returning');
 }
 delete options.payloadSigner;
 cases.push({private:privateSocket,category,demo,options,route,override,disposed,signer,relayEnabled,relayKind:true,calls,events});
}
process.stdout.write(JSON.stringify(cases));
