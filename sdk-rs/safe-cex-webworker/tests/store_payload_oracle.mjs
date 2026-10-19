// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
const worker=ast('webworker/ExchangeWorker.ts'),store=ast('store/store.base.ts');
const cls=worker.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const names=['_shouldSendUpdateIdBackedSnapshot','_recordSentUpdateIdBackedSnapshot','_copyTickerBaseline','_packTickerDeltasSinceBaseline'];
const methods=names.map(name=>{const member=cls.members.find(m=>m.name?.getText(worker)===name);if(!member)throw Error(name);return member.getText(worker);});
const fields=store.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(store)==='PACKED_TICKER_FIELDS')).getText(store).replace('export ','');
const context=vm.createContext({});vm.runInContext(ts.transpileModule(fields+'\nclass Worker { '+cls.members.find(m=>m.name?.getText(worker)==='_lastSentTickerBaseline').getText(worker)+'\n'+methods.join('\n')+' }\nglobalThis.Worker=Worker;globalThis.fields=PACKED_TICKER_FIELDS;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const encode=v=>typeof v==='number'?(Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const workerInstance=new context.Worker(),versions=[];
for(const update of [undefined,0,-0,1,-1,1.5,NaN,Infinity,-Infinity])for(const previous of [null,0,-0,1,-1,1.5,NaN,Infinity,-Infinity])for(const force of [undefined,false,true])versions.push(encode({update,previous,force,send:workerInstance._shouldSendUpdateIdBackedSnapshot(update,previous,force),record:workerInstance._recordSentUpdateIdBackedSnapshot(update)}));
const ticker=(id,symbol)=>Object.fromEntries([['id',id],['symbol',symbol],...context.fields.map((f,i)=>[f,i+1])]);
const base=[ticker('a','BTC'),ticker('b','ETH')];const traces=[];
for(const field of context.fields)for(const value of [0,-0,2,NaN,Infinity,-Infinity]){
 const a=structuredClone(base);a[1][field]=value;
 traces.push([{op:'pack',tickers:base},{op:'copy',tickers:base},{op:'pack',tickers:a},{op:'pack',tickers:a},{op:'pack',tickers:base}]);
}
const changed=structuredClone(base);changed[0].bid=99;changed[1].id='other';
const restored=structuredClone(changed);restored[1].id='b';
const symbol=structuredClone(base);symbol[1].symbol='OTHER';
const untracked=structuredClone(base);untracked[0].info={changed:true};untracked[0].midPrice=101;
traces.push([{op:'copy',tickers:base},{op:'pack',tickers:changed},{op:'pack',tickers:restored},{op:'pack',tickers:symbol},{op:'copy',tickers:symbol},{op:'pack',tickers:symbol},{op:'copy',tickers:base},{op:'pack',tickers:untracked}]);
traces.push([{op:'pack',tickers:[]},{op:'copy',tickers:[]},{op:'pack',tickers:[]},{op:'pack',tickers:base},{op:'copy',tickers:base},{op:'pack',tickers:[base[0]]},{op:'copy',tickers:[base[0]]},{op:'pack',tickers:[base[0]]}]);
const all=structuredClone(base);for(const f of context.fields)all[0][f]=99;
traces.push([{op:'copy',tickers:base},{op:'pack',tickers:all}]);
const plus=structuredClone(base),minus=structuredClone(base);for(const f of context.fields){plus[0][f]=0;minus[0][f]=-0;}
traces.push([{op:'copy',tickers:plus},{op:'pack',tickers:minus},{op:'pack',tickers:plus},{op:'copy',tickers:minus},{op:'pack',tickers:plus},{op:'pack',tickers:minus}]);
const nullIndex=structuredClone(base);nullIndex[0].index=null;
traces.push([{op:'copy',tickers:base},{op:'pack',tickers:nullIndex},{op:'pack',tickers:nullIndex},{op:'pack',tickers:base}]);
const scenarios=traces.map(actions=>{const instance=new context.Worker();return {actions:encode(actions),expected:actions.map(a=>a.op==='copy'?(instance._copyTickerBaseline(a.tickers),null):encode(instance._packTickerDeltasSinceBaseline(a.tickers)))};});
process.stdout.write(JSON.stringify({fields:context.fields,versions,scenarios}));
