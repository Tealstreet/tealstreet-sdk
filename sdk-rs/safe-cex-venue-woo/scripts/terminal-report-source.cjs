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
const methods=['mapLimitOrder'];
const members=methods.map(name=>{const member=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,name);return member.getText(ast)});
const constants=load(path.join(root,'exchanges/woo/woo.types.ts'));
const context={...constants,...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'utils/safe-math.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts')),virtualClock:{getCurrentTimeMs:()=>1234}};
vm.runInNewContext(ts.transpileModule('class Witness {'+members.join('\n')+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const worker=new context.Witness();worker.store={marketsBySymbol:{BTCUSDT:{id:'BTC',symbol:'BTCUSDT'}}};

for(const [type,price] of [['IOC',101],['FOK',101],['MARKET',0]]) {
 const input={symbol:'PERP_BTC_USDT',orderId:811,side:'BUY',type,status:'FILLED',quantity:2,
  price,reduceOnly:false,executedQuantity:0.1,executedPrice:100.5};
 const order=worker.mapLimitOrder(input);
 assert.equal(order.id,'811');assert.equal(order.symbol,'BTCUSDT');assert.equal(order.side,'buy');
 assert.equal(order.type,type==='MARKET'?'market':undefined,'actual table hit '+type);
 assert.equal(order.price,price===0?undefined:price,'actual truthy price fallback '+type);
 assert.equal(order.amount,2);assert.equal(order.remaining,2);assert.equal(order.info,input);
 assert.ok(Object.hasOwn(order,'type'));assert.ok(Object.hasOwn(order,'price'));
}
console.log('Actual TS mapper: IOC/FOK undefined type, MARKET undefined price; all three records returned');
