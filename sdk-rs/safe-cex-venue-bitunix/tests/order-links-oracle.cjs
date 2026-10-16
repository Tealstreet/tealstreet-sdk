'use strict';
const {ts,fs,path,vm,root,assert,load}=require('./oracle-loader.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');assert.equal(owners.length,1);
const owner={store:{orders:[]}};const {OrderType}=load(path.join(root,'packages/safe-cex/src/types.ts'));
for(const name of ['getVirtualTpslLeg','getVirtualTpslParentOrderId','getTpslRawOrderId','getAttachedVirtualTpslLeg','isStandaloneTpslOrder','isPositionTpslOrder']) {
 const members=owners[0].members.filter(n=>n.name?.getText(ast)===name);assert.equal(members.length,1,`ORDER LINK helper ${name}`);assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`(function(OrderType){return (${members[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 owner[name]=vm.runInThisContext(code,{filename:file}).call(owner,OrderType);
}
function rawOrder(id,type,amount='1.25',info) {
 return `{"id":${JSON.stringify(id)},"status":"open","symbol":"BTCUSDT","type":${JSON.stringify(type)},"side":"buy","price":11.125,"amount":${amount},"filled":2.25,"remaining":8.875,"reduceOnly":false,"lastUpdate":17.5${info===undefined?'':`,"info":${info}`}}`;
}
const groups=[[],[['parent',OrderType.Limit]],[['parent',OrderType.TakeProfit]],[['parent',OrderType.TakeProfit],['parent',OrderType.Limit]],[['parent',OrderType.Limit],['parent',OrderType.TakeProfit]],[['parent__tp',OrderType.Market]],[['',OrderType.Limit]]];
const specs=[];
for(const id of ['parent__tp','parent__sl','orphan__tp','plain','parent__tp__sl','parent__TP','__tp'])for(const type of [OrderType.Limit,OrderType.Market,OrderType.TakeProfit,OrderType.StopLoss])for(const group of groups)specs.push({raw:rawOrder(id,type),orders:group.map(([id,type])=>rawOrder(id,type))});
const infos=[undefined,'null','{}','{"id":null,"orderId":"second"}','{"id":"","orderId":"ignored"}','{"id":0,"orderId":"ignored"}','{"id":false}','{"id":[1,2]}','{"id":{"toString":1}}','{"orderId":1e400}','{"type":"position_tpsl"}','{"type":"POSITION_TPSL"}','{"type":" POSITION_TPSL "}','{"type":false}','{"type":0}','{"type":["position_tpsl"]}','{"type":{"toString":1}}','{"id":"\\ud800"}'];
for(const info of infos)for(const amount of ['0','-0','3.75'])specs.push({raw:rawOrder('parent__tp',OrderType.TakeProfit,amount,info),orders:[rawOrder('parent',OrderType.Limit)]});
const obs=fn=>{try{const v=fn();return typeof v==='string'?{type:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:{type:typeof v,...(v===undefined?{}:{value:v})};}catch(e){return {error:e.name};}};
const rows=specs.map(({raw,orders})=>{const order=JSON.parse(raw);owner.store.orders=orders.map(JSON.parse);return {raw,orders,rawId:obs(()=>owner.getTpslRawOrderId(order)),attached:obs(()=>owner.getAttachedVirtualTpslLeg(order)),standalone:obs(()=>owner.isStandaloneTpslOrder(order)),position:obs(()=>owner.isPositionTpslOrder(order))};});
assert.equal(rows.length,250,'ORDER LINK source census');process.stdout.write(JSON.stringify(rows));
