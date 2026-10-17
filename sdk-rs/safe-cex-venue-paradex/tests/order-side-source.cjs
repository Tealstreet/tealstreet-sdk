const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname,'../../..');
const read = relative => fs.readFileSync(path.join(root,relative),'utf8');
const tree = relative => ts.createSourceFile(relative,read(relative),99,true);
function execute(code,globals={}) {
  const module={exports:{}};
  const result=ts.transpileModule(code,{reportDiagnostics:true,compilerOptions:{module:1,target:9}});
  assert.equal(result.diagnostics.length,0);
  vm.runInNewContext(result.outputText,{module,exports:module.exports,...globals});return module.exports;
}
const source=tree('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const names=['fetchOrders','mapOrder','reverseMapOrderType','mapOrderStatus'];
const members=names.map(name=>{const hits=cls.members.filter(n=>n.name?.getText(source)===name);assert.equal(hits.length,1);return hits[0].getText(source);});
const shared=tree('packages/safe-cex/src/types.ts');
const enums=execute(shared.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide','OrderStatus'].includes(n.name.text)).map(n=>n.getText(shared)).join('\n'));
const constants=execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'),{require:()=>enums});
const logging=tree('packages/logging/src/LogSeverity.ts');
const {LogSeverity}=execute(logging.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(logging)).join('\n'));
const describe=source.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(source)==='describeError'));
assert.ok(describe);
const {Harness}=execute(describe.getText(source)+'\nexport class Harness {'+members.join('\n')+'}',{...enums,...constants,LogSeverity});
const row=id=>({id,market:'BTC-USD-PERP',side:'BUY',type:'LIMIT',status:'OPEN',price:'101.5',size:'0.006',remaining_size:'0.003',created_at:1784505600000,reduce_only:false});
function fixture(rows) {
  const h=new Harness();h.jwtManager={};h.sdkInitPromise=null;
  let requests=0;h.xhr={get:async()=>{requests++;return {data:{results:rows}};}};h.log=()=>{};
  return {h,count:()=>requests};
}
(async()=>{
  let checks=0;
  for(const value of [undefined,'unknown','',null,17,false,{},[],'BUY','SELL',['BUY']]) {
    const raw={...row('side'),side:value};if(value===undefined)delete raw.side;
    const {h,count}=fixture([raw]);const result=await h.fetchOrders();
    assert.equal(result.length,1);
    const expected=value==='BUY'||Array.isArray(value)&&value[0]==='BUY'?'buy':value==='SELL'?'sell':undefined;
    assert.equal(result[0].side,expected);
    assert.equal(Object.hasOwn(result[0],'side'),true);
    assert.equal(result[0].info,raw);assert.equal(count(),1);checks++;
  }
  const {h,count}=fixture([{...row('bad'),side:'unknown',created_at:{toString:null}}]);
  await assert.rejects(()=>h.fetchOrders(),error=>error.name==='TypeError'&&error.message==='Cannot convert object to primitive value');
  assert.equal(count(),1);checks++;
  assert.equal(checks,12);process.stdout.write(String(checks));
})().catch(error=>{console.error(error);process.exit(1);});
