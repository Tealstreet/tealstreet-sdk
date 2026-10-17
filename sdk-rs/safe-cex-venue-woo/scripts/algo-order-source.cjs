// Execute actual TypeScript mapper bodies; these observations precede JSON projection.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 let source=fs.readFileSync(file,'utf8');
 if(file===path.join(root,'types.ts')){const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);source=ast.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(ast)).join('\n');}
 const exports={};cache.set(file,exports);
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):name==='@tealstreet/math/src/isNotNumber'?load(path.resolve(root,'../../math/src/isNotNumber.ts')):runtime(name)},{filename:file});
 return exports;
}
const file=path.join(root,'exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const methods=['mapAlgoOrder'];
const members=methods.map(name=>{const member=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,name);return member.getText(ast)});
const constants=load(path.join(root,'exchanges/woo/woo.types.ts'));
const context={...constants,...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'utils/safe-math.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts')),virtualClock:{getCurrentTimeMs:()=>1234}};
vm.runInNewContext(ts.transpileModule('class Witness {'+members.join('\n')+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const worker=new context.Witness();worker.store={marketsBySymbol:{BTCUSDT:{id:'BTC',symbol:'BTCUSDT'}}};

const partial={algoOrderId:7,rootAlgoOrderId:6,symbol:'PERP_BTC_USDT',algoType:'STOP',side:'SELL',triggerPrice:90,triggerPriceType:'MARK_PRICE',quantity:'1.25',totalExecutedQuantity:0.25};
const [stop]=worker.mapAlgoOrder(partial);
assert.equal(stop.id,'7');assert.equal(stop.type,undefined);assert.ok(Object.hasOwn(stop,'type'));
assert.equal(stop.amount,'1.25');assert.equal(stop.remaining,1);assert.equal(stop.reduceOnly,undefined);assert.ok(Object.hasOwn(stop,'reduceOnly'));
assert.equal(stop.info,partial);assert.equal(stop.tpTriggerType,'mark_price');
const parent={symbol:'PERP_BTC_USDT',rootAlgoOrderId:10,childOrders:[
{algoOrderId:11,algoType:'TAKE_PROFIT',side:'SELL',triggerPrice:110,quantity:2,totalExecutedQuantity:0,reduceOnly:true},
{algoOrderId:12,algoType:'STOP_LOSS',side:'SELL',triggerPrice:90,quantity:2,totalExecutedQuantity:0,reduceOnly:true}]};
const rows=[...worker.mapAlgoOrder(partial),...worker.mapAlgoOrder(parent)];
assert.equal(rows.length,3);assert.equal(rows.map(row=>row.id).join(','),'7,11,12');assert.equal(rows[1].info,parent);assert.equal(rows[2].info,parent);
for(const quantity of [null,undefined,false,'2.5']){const row={...partial,quantity};const [mapped]=worker.mapAlgoOrder(row);assert.equal(mapped.amount,quantity);assert.equal(mapped.info,row);}
console.log('Actual TS algo mapper: STOP undefined type, copied string/null/undefined/bool quantity and reduceOnly, three retained stop/bracket orders and live info identities');
