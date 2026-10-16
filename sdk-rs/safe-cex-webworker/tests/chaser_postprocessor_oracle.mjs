// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import assert from 'node:assert/strict';
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
const program=ts.transpileModule(enums+'\n'+['utils/clone.ts','webworker/chaserOrder/ChaserOrderInstance.ts','webworker/postProcessors/ExchangeDependentPostprocessor.ts','webworker/postProcessors/ChaserOrderPostprocessor.ts'].map(source).join('\n')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.Processor=ChaserOrderPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const encode=v=>typeof v==='number'?(Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const cases=[];
for(const reduceOnly of [undefined,false,true])for(const origin of [undefined,'web','cli','webhook','automation'])for(const size of [0,-0,3,NaN,Infinity,-Infinity]){
 let nextId=0;const manager={instances:[]};const context=vm.createContext({chaserOrderManagerInstance:manager,Date:{now:()=>0},nanoid:()=> `id${nextId++}`,structuredClone});vm.runInContext(program,context);
 const venues=[{name:'bybit',store:{positions:[]}},{name:'bybit',store:{positions:[]}}];
 const specs=[{venue:0,side:'buy',symbol:'BTC',attempts:3,ids:['first','first']},{venue:1,side:'sell',symbol:'OTHER',attempts:5,ids:['other']},{venue:0,side:'sell',symbol:'STOPPED',attempts:0,ids:[],stopped:true},{venue:0,side:'sell',symbol:'ETH',attempts:-1,ids:['last']}];
 manager.instances=specs.map(s=>{const i=new context.Instance(venues[s.venue],{symbol:s.symbol,side:s.side,size,maxAttempts:17.25,delay:100,reduceOnly,origin});i.state.attempts=s.attempts;i.currentOrderIds=s.ids;if(s.stopped)i.markComplete('stopped');return i;});
 const store={activeChasers:[{id:'stale'}]};const processor=new context.Processor('a','bybit',{},venues[0]);processor.process(store);const first=encode(store.activeChasers);
 manager.instances[0].state.currentOrdersIds.push('late');const aliased=store.activeChasers[0].currentOrderIds.includes('late');
 manager.instances[0].options.symbol='UPDATED';manager.instances[0].state.attempts=9;processor.process(store);const second=encode(store.activeChasers);
 manager.instances=[];processor.process(store);
 cases.push({reduceOnly,origin,size:encode(size),first,second,cleared:store.activeChasers,aliased});
}
const declaration=types.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='ActiveChaser');
assert.ok(declaration&&ts.isTypeLiteralNode(declaration.type),'CHASER-IDENTITY supported source record');
const fields=declaration.type.members.map(n=>{assert.ok(ts.isPropertySignature(n)&&n.type,'CHASER-IDENTITY source property');return {name:n.name.getText(types),type:n.type.getText(types)};});
assert.deepEqual(fields.map(f=>f.name).sort(),['id','symbol','size','side','maxAttempts','reduceOnly','attempts','isStopped','stopReason','currentOrderIds','origin'].sort(),'CHASER-IDENTITY all eleven native fields inventoried');
const records=cases.flatMap(c=>[...c.first,...c.second]);
assert.equal(records.length,360,'CHASER-IDENTITY projected record inventory');
let pairs=0;
for(let i=0;i<fields.length;i++)for(let j=i+1;j<fields.length;j++){
 const a=fields[i],b=fields[j];
 if(a.type!==b.type)continue;
 pairs++;
 assert.ok(records.some(r=>Object.hasOwn(r,a.name)&&Object.hasOwn(r,b.name)&&!Object.is(r[a.name],r[b.name])),`CHASER-IDENTITY distinguish ${a.name}/${b.name}`);
}
assert.equal(pairs,7,'CHASER-IDENTITY compatible source field pairs');
process.stdout.write(JSON.stringify(cases));
