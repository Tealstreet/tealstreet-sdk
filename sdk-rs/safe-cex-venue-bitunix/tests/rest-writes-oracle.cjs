'use strict';
const {ts,fs,path,vm,root,assert,load}=require('./oracle-loader.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');assert.equal(owners.length,1,'WRITE owner');
const types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const ENDPOINTS=load(path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts')).ENDPOINTS;
const createPositionId=load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts')).createPositionId;
const storeFile=path.join(root,'packages/safe-cex/src/store/store.base.ts'),storeAst=ts.createSourceFile(storeFile,fs.readFileSync(storeFile,'utf8'),ts.ScriptTarget.ES2022,true);
const terminal=storeAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['TERMINAL_STATUSES','isTerminalOrder'].includes(d.name.getText(storeAst))));assert.equal(terminal.length,2,'WRITE terminal selector');
const isTerminalOrder=vm.runInNewContext(ts.transpileModule(terminal.map(n=>n.getText(storeAst).replace(/^export /,'')).join('\n')+'\nisTerminalOrder;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{OrderStatus:types.OrderStatus});
const arrows={};
for(const name of ['changePositionMode','changeMarginType','setLeverage','cancelSymbolOrders','placeOrders']) {
 const members=owners[0].members.filter(n=>n.name?.getText(ast)===name);assert.equal(members.length,1,`WRITE method ${name}`);
 arrows[name]=ts.transpileModule(`(function(){return (${members[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
const fromBits=s=>Buffer.from(s,'hex').readDoubleBE();
function obs(v){
 if(v===undefined)return {type:'undefined'};if(v===null)return {type:'null'};
 if(typeof v==='number')return {type:'number',bits:bits(v)};
 if(typeof v==='string')return {type:'string',units:units(v)};
 if(typeof v==='boolean')return {type:'boolean',value:v};
 if(Array.isArray(v))return {type:'array',items:v.map(obs)};
 return {type:'object',entries:Object.entries(v).map(([k,v])=>[k,obs(v)])};
}
const BTC=units('BTCUSDT'),ETH=units('ETHUSDT');
const modes=[{hedged:true},{hedged:false},{hedged:true,contractsBits:[bits(1)]},{hedged:false,contractsBits:[bits(-1),bits(-0),bits(NaN)]},{hedged:true,postFail:true},{hedged:false,settingFail:true},{hedged:true,contractsBits:[bits(1)],emitFail:true},{hedged:true,postFail:true,emitFail:true}];
const margins=[{isolated:true,symbolUnits:BTC},{isolated:false,symbolUnits:units('é/😀')},{isolated:true,symbolUnits:[0xd800]},{isolated:true,symbolUnits:[]},{isolated:false},{isolated:false,symbolUnits:BTC,postFail:true},{isolated:true,symbolUnits:BTC,postFail:true,falseMessage:true},{isolated:true,emitFail:true},{isolated:true,symbolUnits:BTC,postFail:true,emitFail:true}];
const leverages=[{inputBits:bits(5)},{inputBits:bits(7)},{inputBits:bits(0)},{inputBits:bits(101)},{inputBits:bits(NaN)},{inputBits:bits(-0),minBits:bits(-0),maxBits:bits(0),currentBits:bits(1)},{inputBits:bits(7),noMarket:true},{inputBits:bits(7),noPosition:true},{inputBits:bits(7),postFail:true},{inputBits:bits(7),updateFail:true},{inputBits:bits(7),postFail:true,emitFail:true}];
const cancels=[{items:[0,1,2],symbolUnits:BTC},{items:[],symbolUnits:BTC},{items:[0,1,2],symbolUnits:[0xd800],surrogate:true},{items:[0,null,2],symbolUnits:BTC},{items:[0,1],symbolUnits:BTC,getterFail:0},{items:[0,1],symbolUnits:BTC,cancelFail:true},{items:[0,1,2],symbolUnits:BTC,deleteOnGet:true}];
const batches=[{items:[]},{items:[0,1]},{items:[0,1],assertFail:true},{items:[0,1],hydrateFail:0},{items:[0,1],formatFail:0},{items:[0,1],batchFail:true},{items:[0,1],appendHydrate:true},{items:[0,null,1]},{items:[0,1],appendFormat:true}];
assert.deepEqual([modes.length,margins.length,leverages.length,cancels.length,batches.length],[8,9,11,7,9],'WRITE independent family counts');
async function run(method,fixture){
 const events=[];const record=(...v)=>events.push(v.map(obs));
 const err=(message,id)=>Object.assign(new Error(message),{id});
 const postError=err('transport failed','post');postError.response={data:{msg:fixture.falseMessage?false:'venue failed'}};
 const emit=message=>{record('emit',message);if(fixture.emitFail)throw err('emit failed','emit');};
 const positions=(fixture.contractsBits??[]).map(contracts=>({contracts:fromBits(contracts)}));
 const owner={store:{positions,setSetting:(key,value)=>{record('setting',key,value);if(fixture.settingFail)throw err('setting failed','setting');},updatePositions:updates=>{record('updates',updates);if(fixture.updateFail)throw err('update failed','update');}},emitAndLogError:emit,xhr:{post:async(url,body)=>{record('post',url,body);if(fixture.postFail)throw postError;return {accepted:true};}}};
 const context={ENDPOINTS,PositionSide:types.PositionSide,createPositionId,isTerminalOrder};
 owner[method]=vm.runInNewContext(arrows[method],context,{filename:file}).call(owner);
 let ack={ack:true},result,error;
 try{
 if(method==='changePositionMode')result=await owner[method](fixture.hedged);
 else if(method==='changeMarginType')result=await owner[method](fixture.isolated,fixture.symbolUnits===undefined?undefined:String.fromCharCode(...fixture.symbolUnits));
 else if(method==='setLeverage'){
  const market=fixture.noMarket?undefined:{limits:{leverage:{min:fromBits(fixture.minBits??bits(1)),max:fromBits(fixture.maxBits??bits(100))}}};
  Object.defineProperty(owner.store,'marketsBySymbol',{get:()=>{record('market lookup');return {BTCUSDT:market};}});
  Object.defineProperty(owner.store,'positions',{get:()=>{record('position lookup');return fixture.noPosition?[]:[{symbol:'BTCUSDT',leverage:fromBits(fixture.currentBits??bits(5))}];}});
  result=await owner[method]('BTCUSDT',fromBits(fixture.inputBits));
 }else if(method==='cancelSymbolOrders'){
  const orders=fixture.items.map(id=>id===null?undefined:{id});fixture.items.forEach((id,i)=>{if(id===null)delete orders[i];});
  for(const order of orders){if(!order)continue;Object.defineProperty(order,'symbol',{get:()=>{record('symbol',order.id);if(fixture.getterFail===order.id)throw err('getter failed','getter');if(fixture.deleteOnGet&&order.id===0)delete orders[1];return String.fromCharCode(...(order.id===1?ETH:fixture.surrogate?[0xd800]:BTC));}});}
  owner.store.orders=orders;owner.cancelOrders=async selected=>{record('cancel',selected.map(o=>o.id));if(fixture.cancelFail)throw err('cancel failed','cancel');return ack;};
  result=await owner[method](String.fromCharCode(...fixture.symbolUnits));
 }else{
  const orders=fixture.items.map(id=>id===null?undefined:{id});fixture.items.forEach((id,i)=>{if(id===null)delete orders[i];});
  owner.assertCallerClientOrderIds=received=>{assert.equal(received,orders,'WRITE input array identity');record('assert');if(fixture.assertFail)throw err('assert failed','assert');};
  owner.hydratePositionIdBeforeFormatting=async order=>{record('hydrate',order?.id);if(fixture.hydrateFail!==undefined&&fixture.hydrateFail===order?.id)throw err('hydrate failed','hydrate');if(fixture.appendHydrate&&order?.id===0)orders.push({id:2});};
  owner.formatCreateOrder=order=>{record('format',order.id);if(fixture.formatFail===order.id)throw err('format failed','format');if(fixture.appendFormat&&order.id===0)orders.push({id:2});return Array.from({length:order.id+1},(_,i)=>`${order.id}:${i}`);};
  owner.placeOrderBatch=async requests=>{record('batch',requests);if(fixture.batchFail)throw err('batch failed','batch');return ack;};
  result=await owner[method](orders);
 }
 }catch(e){error={name:e.name,message:units(e.message??''),id:e.id??null};}
 return {method,fixture,events,error:error??null,returned:obs(result),ackIdentity:result===ack};
}
(async()=>{
 const rows=[];
 for(const [method,fixtures] of [['changePositionMode',modes],['changeMarginType',margins],['setLeverage',leverages],['cancelSymbolOrders',cancels],['placeOrders',batches]])for(const fixture of fixtures)rows.push(await run(method,fixture));
 assert.equal(rows.length,44,'WRITE independent source total');process.stdout.write(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
