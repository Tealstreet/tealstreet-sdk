const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const enums={};
const typeSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');
const ast=ts.createSourceFile('types.ts',typeSource,ts.ScriptTarget.Latest,true);
vm.runInNewContext(ts.transpileModule(ast.statements.filter(ts.isEnumDeclaration).map(s=>s.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
const exportsObject={};
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/journal/woo.position-adapter.ts'),'utf8');
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject,require:name=>{assert.equal(name,'../types');return enums;}});
// Base witness transcribed from the committed woo.position-adapter.test.ts fixture.
const base={id:'1',symbol:'ETHUSDT',side:enums.OrderSide.Buy,type:enums.OrderType.Limit,status:enums.OrderStatus.Closed,price:1900,averagePrice:1876.6,amount:0.0108,filled:0.0108,reduceOnly:false,positionSide:'both',createdTime:1786690690000,updatedTime:1786690690847,realizedPnl:0,fees:0.00182406,feeAsset:'USDT'};
const cases=[];
const add=(name,orders)=>cases.push({name,orders,expected:exportsObject.wooOrdersToExecutions(orders)});
add('base',[base]);
for(const [field,values] of Object.entries({filled:[0,-1,0.005],averagePrice:[undefined,0,-1,1900],updatedTime:[undefined,0,-1,1001],realizedPnl:[undefined,0,-10,10],fees:[undefined,0,-0.001,0.001],positionSide:[undefined,'both','long','short'],side:['buy','sell']}))
 for(const value of values)add(`${field}=${value}`,[{...base,[field]:value}]);
add('cancelled-unfilled-excluded',[base,{...base,id:'cancelled',status:enums.OrderStatus.Canceled,filled:0}]);
add('partial-cancelled-still-execution',[{...base,id:'partial',status:enums.OrderStatus.Canceled,filled:0.005}]);
add('open-close-one-per-order',[{...base,id:'open',filled:1,averagePrice:100,updatedTime:1000000},{...base,id:'close',side:enums.OrderSide.Sell,filled:1,averagePrice:110,updatedTime:2000000,realizedPnl:10}]);
const target=path.join(__dirname,'../tests/fixtures/journal.json');
const bytes=JSON.stringify(cases,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO journal projection drift');
else fs.writeFileSync(target,bytes);
console.log(`WOO journal live oracle: ${cases.length} cases; order projection only, no private grading`);
