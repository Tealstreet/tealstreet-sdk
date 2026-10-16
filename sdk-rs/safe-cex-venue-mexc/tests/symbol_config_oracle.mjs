import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts,boundaries} from './source.mjs';
// Inject the same host clock boundary as the other real-Store oracles.
boundaries.set(path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock:{getCurrentTimeMs:()=>1000}});
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const names=['deriveSymbolConfig','getLeverageForOrder'];
const fields=[];
for(const node of ast.statements)if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')
 for(const member of node.members)if(names.includes(member.name?.getText(ast)))fields.push('this.'+member.name.getText(ast)+'='+member.initializer.getText(ast)+';');
if(fields.length!==2)throw Error('symbol config source census');
let source=fields.join('\n');
if(process.env.MEXC_TS_CONTROL==='config-oneway'){
 const before='positionMode === PositionMode.OneWay && position';
 if(!source.includes(before))throw Error('missing one-way config control anchor');
 source=source.replace(before,'false');
}
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const bindings={...load('packages/safe-cex/src/types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts')};
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const number=n=>{if(Number.isNaN(n))return {$number:'7ff8000000000000'};const b=Buffer.alloc(8);b.writeDoubleBE(n);return {$number:b.toString('hex')};};
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?number(v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const position=(side='long',extra={})=>({id:side,symbol:'BTCUSDT',side,contracts:1,leverage:7,...extra});
const positions=[
 [],[position()],[position('short')],[position(),position('short',{leverage:9})],
 [position('short',{leverage:9}),position('long',{leverage:11})],
 [position('long',{contracts:0}),position('short',{leverage:9})],
 [position('long',{contracts:-0})],[position('long',{contracts:NaN})],
 [position('long',{contracts:Infinity,leverage:7})],
 [position('long',{contracts:-Infinity,leverage:-7})],
 [position('long',{contracts:-1,leverage:NaN})],
 [position('long',{leverage:-0})],
 [position('short',{symbol:'ETHUSDT',leverage:33}),position('long',{positionMode:'oneway'})],
 [position('short',{positionMode:'oneway',leverage:12})],
 [position('long',{positionMode:'hedged',marginType:'isolated'})],
 [position('long',{info:{openType:1}})],
 [position('long',{marginType:'isolated',info:{openType:null}})],
 [position('long',{info:{openType:{toString:null}}})],
];
const existing=[undefined,{marginType:'cross',buyLeverage:0,sellLeverage:40,positionMode:'hedged'},
 {marginType:'isolated',buyLeverage:NaN,sellLeverage:Infinity,positionMode:'oneway'},
 {marginType:'cross',buyLeverage:-0,sellLeverage:0,positionMode:'oneway'},
 {marginType:'isolated',buyLeverage:3,sellLeverage:6,positionMode:'hedged'}];
const cases=[];
for(const rows of positions)for(const accountMode of ['hedged','oneway'])for(const config of existing){
 const store=new DefaultStore();store.update({positions:structuredClone(rows)});
 const context=vm.createContext({...bindings,store,accountPositionMode:accountMode,symbolSettings:new Map(config?[['BTCUSDT',config]]:[])});
 vm.runInContext(js,context,{filename});
 const invoke=(method,opts)=>{try{return {output:encode(context[method](opts))};}catch(e){return {error:{name:e.name,message:e.message}};}};
 const getters=[];
 for(const side of ['buy','sell'])for(const reduceOnly of [false,true])getters.push({side,reduceOnly,...invoke('getLeverageForOrder',{symbol:'BTCUSDT',side,reduceOnly})});
 cases.push({rows:encode(rows),accountMode,config:encode(config),...invoke('deriveSymbolConfig','BTCUSDT'),getters});
}
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
