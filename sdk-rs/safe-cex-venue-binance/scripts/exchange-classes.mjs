import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const read=path=>ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
const compile=text=>ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const output=[];
for(const [stem,name] of [['binance','BinanceExchange'],['binancepm','BinancePMExchange'],['binancespot','BinanceSpotExchange']]) {
  const ast=read(`packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`);
  const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);
  assert.ok(klass,'actual exchange class exists');
  const clients={route:{label:'route'},primary:{label:'primary'},secondary:{label:'secondary'},API:{label:'API'},PAPI:{label:'PAPI'},FAPI:{label:'FAPI'},PublicAPI:{label:'PublicAPI'}};
  const opts={key:'key',secret:'secret'},store={orders:[],positions:[],balance:{total:Infinity,free:NaN,used:-0,currencies:{witness:{total:-Infinity,free:NaN,used:-0}}}},guards=[];
  for(const client of Object.values(clients)) client.interceptors={response:{use:(good,bad)=>guards.push({good,bad})}};
  let receiver;
  class BaseExchange {
    constructor(n,o,s){assert.equal(n,stem);assert.equal(o,opts);assert.equal(s,store);this.options=o;this.store=s;receiver=this;}
    createXhrWithSecondaryKey(o,f,limits){return split(o,this,f,limits)}
    installRequestWeightGuards(rows){guards.push(...rows)}
  }
  class Child {constructor(parent){assert.equal(parent,receiver);this.parent=parent}}
  const factory=label=>(o,parent)=>{assert.equal(o,opts);assert.equal(parent,receiver);return clients[label]};
  const createAPI=factory('API'),createPAPI=factory('PAPI'),createFAPI=factory('FAPI'),createPublicAPI=factory('PublicAPI');
  const split=(o,parent,f,limits)=>{
    assert.equal(o,opts);assert.equal(parent,receiver);
    assert.equal(f,stem==='binancepm'?createPAPI:createAPI,'actual exchange split factory');
    if(limits){assert.equal(limits.rateLimit(clients.primary),clients.primary)}
    return stem==='binancespot'?clients.route:[clients.route,clients.primary,clients.secondary];
  };
  const context={exports:{},BaseExchange,createAPI,createPAPI,createFAPI,createPublicAPI,createApiWithSecondaryKey:split,
    rateLimit:(client,options)=>{assert.equal(options.maxRPS,3);return client},
    patternClientOrderIdRule:(label,pattern,extra)=>({label,pattern,extra}),uuid:()=> 'fixture',
    GRAFANA_ENDPOINTS_EVERY:[],GRAFANA_ENDPOINTS_PERIODIC:[], Date:{now:()=>20000},
    BinancePublicWebsocket:Child,BinancePrivateWebsocket:Child,BinancePMPublicWebsocket:Child,BinancePMPrivateWebsocket:Child};
  if(stem==='binance'){
    const declarations=ast.statements.filter(node=>ts.isVariableStatement(node)&&node.declarationList.declarations.some(declaration=>declaration.name.getText(ast).startsWith('REQUEST_WEIGHT_')));
    assert.equal(declarations.length,3,'actual source request-weight declaration floor stops at REQUEST_WEIGHT_BACKOFF_MS');
    vm.runInNewContext(compile(declarations.map(node=>node.getText(ast)).join('\n')),context);
  }
  vm.runInNewContext(compile(process.env.SAFE_CEX_CLASS_SOURCE_CONTROL==='exchange-client'?klass.getText(ast).replace('this.fapiXhr = createFAPI(opts, this);','this.fapiXhr = createPAPI(opts, this);'):klass.getText(ast)),context);
  let owner=new context.exports[name](stem,opts,store);
  assert.equal(owner.publicWebsocket.parent,owner,'actual public child receives original exchange');
  assert.equal(owner.privateWebsocket.parent,owner,'actual private child receives original exchange');
  if(stem==='binance'){
    const empty={};
    assert.equal(guards[0].good(empty),empty,'actual missing status fulfills unchanged without serialization');
    const error={response:{}};
    await guards[0].bad(error).then(()=>assert.fail('actual rejected guard must reject'),caught=>assert.equal(caught,error,'actual missing rejection status retains original error'));
    guards[0].good({status:'429',headers:{'RETRY-AFTER':2.5}});
    assert.equal(owner.backgroundReconciliationPausedUntil,0,'actual string 429 does not select numeric retry branch');
    let serializationReads=0;
    const callable=function nativeStatus(){};
    Object.defineProperty(callable,'toJSON',{get(){serializationReads++;throw Error('status must not serialize')}});
    const callableResponse={status:callable};
    assert.equal(guards[0].good(callableResponse),callableResponse,'actual callable status fulfills unchanged');
    assert.equal(serializationReads,0,'actual strict status comparison never reads toJSON');
    assert.equal(owner.backgroundReconciliationPausedUntil,0,'actual callable status leaves reconciliation pause unchanged');
    guards[0].good({status:429,headers:{'RETRY-AFTER':2.5}});
    assert.equal(owner.backgroundReconciliationPausedUntil,22500,'actual uppercase header pauses reconciliation 2500ms');
    guards[0].good({status:'429',headers:{'x-mbx-used-weight-1m':1920}});
    assert.equal(owner.backgroundReconciliationPausedUntil,80000,'actual nonnumeric status still observes used weight 1920');
  }
  const fields=stem==='binance'?['xhr','_primaryXhr','_secondaryXhr','publicXhr','unlimitedXHR']:stem==='binancepm'?['papiXhr','_primaryXhr','_secondaryXhr','fapiXhr','unlimitedXHR','publicXhr']:['xhr','unlimitedXHR'];
  const mapped=Object.fromEntries(fields.map(field=>[field,owner[field].label]));
  if(stem==='binancepm') assert.equal(mapped.fapiXhr,'FAPI','actual PM FAPI client remains primary-only FAPI factory');
  assert.equal(mapped[stem==='binancepm'?'papiXhr':'xhr'],'route','actual exchange uses routed private client');
  const before=owner.options.key;opts.key='changed';
  assert.equal(owner.options.key,'changed','actual exchange options retain caller alias');
  const balance=owner.store.balance;
  assert.equal(typeof balance.total,'number','actual source balance total is native number');
  assert.equal(balance.total,Infinity,'actual source owner preserves native Infinity before serialization');
  assert.ok(Number.isNaN(balance.free),'actual source owner preserves native NaN');
  assert.ok(Object.is(balance.used,-0),'actual source owner preserves native negative zero');
  assert.equal(balance.currencies.witness.total,-Infinity,'actual source nested balance preserves negative Infinity');
  assert.ok(Number.isNaN(balance.currencies.witness.free),'actual source nested balance preserves native NaN');
  assert.ok(Object.is(balance.currencies.witness.used,-0),'actual source nested balance preserves native negative zero');
  balance.total=4.2;
  assert.equal(owner.store.balance.total,4.2,'actual exchange balance retains live alias');
  output.push({stem,clients:mapped,guards:guards.length,limit:owner.clientOrderIdRule.pattern.test('x'.repeat(stem==='binancepm'?32:36)),tooLong:owner.clientOrderIdRule.pattern.test('x'.repeat(stem==='binancepm'?33:37)),sameParent:true,optionsBefore:before,optionsAfter:owner.options.key,balanceAfter:owner.store.balance.total});
  const retainedChild=owner.publicWebsocket;
  const privateSibling=new WeakRef(owner.privateWebsocket);
  owner=null;receiver=undefined;
  assert.equal(retainedChild.parent.privateWebsocket,privateSibling.deref(),'actual source retained child reaches private sibling after owner binding clears');
}
assert.equal(output.length,3,'independent exchange class source floor stops at case2');
process.stdout.write(JSON.stringify(output));
