// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
const encode=v=>typeof v==='number'?(Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const traces=[
 [{op:'sub',id:'3',n:1e16},{op:'sub',id:'1',n:-1e16},{op:'sub',id:'2',n:1}],
 [{op:'size',n:-0},{op:'recalc'},{op:'size',n:NaN},{op:'recalc'},{op:'size',n:Infinity},{op:'recalc'}],
 [{op:'size',n:0.158},{op:'set',id:'a',n:0.1},{op:'set',id:'b',n:0.058},{op:'recalc'},{op:'size',n:-0.158},{op:'recalc'},{op:'size',n:0.159},{op:'recalc'}],
 [{op:'size',n:1e12},{op:'set',id:'a',n:1e12-1e3},{op:'recalc'},{op:'set',id:'a',n:1e12-1e2},{op:'recalc'}],
 [{op:'attempt'},{op:'attempt'},{op:'attempt'},{op:'attempt'},{op:'complete'},{op:'complete',reason:''},{op:'claim'},{op:'claim'}],
 [{op:'ids',ids:['a','b','a']},{op:'ids',ids:[]},{op:'ids',ids:['b','c']},{op:'flags',min:-1.5,pending:true,fresh:true},{op:'flags',min:NaN,pending:false,fresh:false},{op:'flags',min:2,pending:true,fresh:false},{op:'flags',min:3,pending:false,fresh:true}],
 [{op:'sub',id:'a',n:3},{op:'sub',id:'a',n:2},{op:'add',id:'a',n:1},{op:'set',id:'a',n:2},{op:'set',id:'a',n:12},{op:'sub',id:'a',n:1},{op:'recalc'},{op:'size',n:3},{op:'recalc'}],
 [{op:'sub',id:'a',n:10,placement:0},{op:'sub',id:'a',n:10,placement:2},{op:'sub',id:'a',n:1,placement:NaN},{op:'sub',id:'a',n:0,placement:-1}],
 [{op:'add',id:'b',n:NaN},{op:'add',id:'b',n:Infinity},{op:'add',id:'b',n:-1},{op:'add',id:'b',n:0},{op:'set',id:'b',n:Infinity},{op:'sub',id:'b',n:NaN},{op:'add',id:'b',n:1}],
 [{op:'sub',id:'z',n:1e16},{op:'sub',id:'2',n:1},{op:'sub',id:'1',n:-1e16},{op:'sub',id:'01',n:2},{op:'sub',id:'4294967295',n:4},{op:'sub',id:'4294967294',n:-2}],
 [{op:'register',label:'first',throws:true},{op:'register',label:'second',register:'late'},{op:'dispose'},{op:'dispose'},{op:'dispose'}],
 [{op:'register',label:'recursive',reenter:true},{op:'register',label:'second'},{op:'dispose'}],
];
for(const id of Object.getOwnPropertyNames(Object.prototype))traces.push([{op:'set',id,n:3},{op:'add',id,n:1},{op:'add',id,n:2},{op:'set',id,n:12},{op:'sub',id,n:1},{op:'add',id,n:2}]);
for(const side of ['buy','sell'])for(const kind of ['price','percent','usd'])for(const value of [-1,0,1,NaN,Infinity])traces.push([{op:'termination',side,kind,value,anchor:100,prices:[-1,-0,0,1,99,100,101,Infinity,NaN]}]);
const program=ts.transpileModule(enums+'\n'+source('utils/clone.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderInstance.ts')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.defaultState=defaultChaserState;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
if(process.argv.includes('--default-snapshot')){
 const context=vm.createContext({Date:{now:()=>0},nanoid:()=> 'id',structuredClone});vm.runInContext(program,context);
 const template=context.defaultState;const ids=template.currentOrdersIds;
 const exchange={store:{positions:[]}},options={symbol:'BTC',side:'buy',size:1,maxAttempts:1,delay:100};
 const before=new context.Instance(exchange,options);
 ids.push('seed');const first=new context.Instance(exchange,options);
 ids.push('later');const second=new context.Instance(exchange,options);
 first.state.currentOrdersIds.push('local');
 const frozen=Object.isFrozen(template), nestedFrozen=Object.isFrozen(ids);
 const replace=Reflect.set(template,'currentOrdersIds',[]), extend=Reflect.set(template,'extra',true);
 const result={before:[...before.currentOrderIds],first:[...first.currentOrderIds],second:[...second.currentOrderIds],template:[...ids],frozen,nestedFrozen,replace,extend};
 ids.length=0;
 process.stdout.write(JSON.stringify(result));process.exit(0);
}
if(process.argv.includes('--representation-witness')){
 const context=vm.createContext({Date:{now:()=>0},nanoid:()=> 'id',structuredClone});vm.runInContext(program,context);
 context.defaultState.currentOrdersIds.push('seed');
 const instance=new context.Instance({store:{positions:[]}},{symbol:'BTC',side:'buy',size:1,maxAttempts:1,delay:100});
 const seeded=[...instance.currentOrderIds];const supplied=['a'];instance.currentOrderIds=supplied;supplied.push('caller');
 const afterCaller=[...instance.currentOrderIds];const held=instance.currentOrderIds;instance.state.currentOrdersIds.push('state');
 process.stdout.write(JSON.stringify({seeded,afterCaller,held}));process.exit(0);
}
const scenarios=[];
for(const side of ['buy','sell'])for(const contracts of [undefined,-3,NaN])for(const reduceOnly of [false,true])for(const actions of traces){
 let now=0;const log=[];const context=vm.createContext({Date:{now:()=>now},nanoid:()=> 'id',structuredClone});
 vm.runInContext(program,context);
 const options={symbol:'BTC',size:10,side,maxAttempts:1,delay:100,reduceOnly};
 const positions=contracts===undefined?[]:[{symbol:'BTC',side:side==='buy'?'long':'short',contracts},{symbol:'BTC',side:side==='buy'?'long':'short',contracts:999}];
 const instance=new context.Instance({store:{positions}},options),heldState=instance.state,heldOptions=instance.options,heldEver=instance.everTrackedOrderIds;
 const snapshot=()=>encode({id:instance.id,state:instance.state,remaining:instance.remainingSize,filled:instance.filledSize,partial:instance.lastPartialFillAt,target:instance.targetPositionContracts??null,ids:instance.currentOrderIds,ever:[...instance.everTrackedOrderIds],stopped:instance.isStopped,min:instance.minTickDistance,pending:instance.pendingOrdersUpdate,fresh:instance.freshOrder,size:instance.options.size,identity:heldState===instance.state&&heldOptions===instance.options&&heldEver===instance.everTrackedOrderIds});
 const expected=[snapshot()];
 for(const action of actions){now+=10;let result=null;
 switch(action.op){
 case 'attempt':instance.incrementAttempts();break;case 'complete':instance.markComplete(action.reason);break;
 case 'claim':result=instance.claimCompleteEmission();break;case 'ids':instance.currentOrderIds=action.ids;break;
 case 'flags':instance.minTickDistance=action.min;instance.pendingOrdersUpdate=action.pending;instance.freshOrder=action.fresh;break;
 case 'sub':instance.subtractFilledSize({id:action.id,filled:action.n,extra:{filledInPlacementUnits:action.placement}});break;
 case 'add':instance.addFillAmount(action.id,action.n);break;case 'set':instance.setFilledAmount(action.id,action.n);break;
 case 'recalc':instance.recalculateRemainingSize();break;case 'size':options.size=action.n;break;
 case 'register':instance.registerDisposer(()=>{log.push(action.label);if(action.register)instance.registerDisposer(()=>log.push(action.register));if(action.reenter)instance.dispose();if(action.throws)throw new Error('disposer failed');});break;
 case 'dispose':instance.dispose();break;
 case 'termination':options.side=action.side;options.terminate={type:action.kind,value:action.value,anchorPrice:action.anchor};result=action.prices.map(p=>instance.shouldTerminate(p));break;
 default:throw new Error('unknown action');}
 expected.push({state:snapshot(),result,log:[...log]});
 }
 scenarios.push(encode({side,contracts,reduceOnly,actions,expected}));
}
process.stdout.write(JSON.stringify(scenarios));
