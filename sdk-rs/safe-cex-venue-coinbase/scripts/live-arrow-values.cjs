const assert=require('node:assert/strict');
const {setup}=require('./live-class-policy.cjs');
(async()=>{
 const cases=[];
 {
  let origin=setup({options:{key:'fixture-key',secret:'fixture-secret'}});
  const target=setup({options:{}}), weak=new WeakRef(origin.exchange);
  origin.exchange.selectedPortfolio={uuid:'origin-cfm',type:'CFM'};
  origin.exchange.supportSpot=false;
  target.exchange.getSupportedTypes=origin.exchange.getSupportedTypes;
  origin=null;
  const types=await target.exchange.getSupportedTypes();
  assert.deepEqual([...types],['cfm'],'source transplanted supported arrow retains original receiver');
  assert.equal(target.exchange.getSupportedTypes,weak.deref().getSupportedTypes,'source transplanted function identity');
  assert.equal(target.exchange.selectedPortfolio,null,'source target selection remains untouched');
  cases.push({id:'supported-arrow-retains-origin',types:[...types],sameFunction:true,targetSelected:null});
 }
 {
  let origin=setup({options:{key:'fixture-key',secret:'fixture-secret'}});
  const target=setup({options:{key:'fixture-key',secret:'fixture-secret'}});
  origin.portfolios[0].uuid='origin-intx';target.portfolios[0].uuid='target-intx';
  const portfolios=origin.portfolios,trace=origin.trace,weak=new WeakRef(origin.exchange);
  target.exchange.findSelectedPortfolio=origin.exchange.findSelectedPortfolio;
  origin=null;
  const selected=await target.exchange.findSelectedPortfolio();
  assert.equal(selected.uuid,'origin-intx','source transplanted finder uses origin HTTP client');
  assert.equal(selected,portfolios[0],'source transplanted finder retains exact portfolio root');
  assert.deepEqual(trace,[{endpoint:'/api/v3/brokerage/portfolios',uuid:undefined},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'origin-intx'},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'ordinary'}]);
  assert.deepEqual(target.trace,[],'source target HTTP client stays unused');
  assert.equal(target.exchange.findSelectedPortfolio,weak.deref().findSelectedPortfolio);
  assert.equal(target.exchange.selectedPortfolio,null);
  cases.push({id:'finder-arrow-retains-origin',selectedUuid:selected.uuid,samePortfolio:true,sameFunction:true,trace,targetCalls:0,targetSelected:null});
 }
 {
  let origin=setup({options:{key:'fixture-key',secret:'fixture-secret'}});
  const target=setup({options:{}}),weak=new WeakRef(origin.exchange);
  origin.exchange.selectedPortfolio={uuid:'origin-cfm',type:'CFM'};origin.exchange.supportSpot=false;
  target.exchange.getSupportedTypes=origin.exchange.getSupportedTypes;
  origin=null;
  const retained=weak.deref();assert(retained,'source bound arrow retains origin');
  retained.options={};const without=await target.exchange.getSupportedTypes();assert.deepEqual([...without],['intx']);
  retained.options={key:'fixture-key',secret:'fixture-secret'};const withCredentials=await target.exchange.getSupportedTypes();assert.deepEqual([...withCredentials],['cfm']);
  assert.equal(target.exchange.selectedPortfolio,null);
  cases.push({id:'transplanted-arrow-reads-origin-option-slot',without:[...without],withCredentials:[...withCredentials],targetSelected:null});
 }
 {
  const origin=setup({options:{key:'fixture-key',secret:'fixture-secret'}}),target=setup({options:{}}),replacement=setup({spot:true,options:{}});
  origin.exchange.selectedPortfolio={uuid:'origin-cfm',type:'CFM'};origin.exchange.supportSpot=false;
  replacement.exchange.selectedPortfolio={uuid:'replacement-cfm',type:'CFM'};
  const original=origin.exchange.getSupportedTypes;
  target.exchange.getSupportedTypes=original;
  origin.exchange.getSupportedTypes=replacement.exchange.getSupportedTypes;
  const intermediate=await origin.exchange.getSupportedTypes();assert.deepEqual([...intermediate],['spot']);
  origin.exchange.getSupportedTypes=target.exchange.getSupportedTypes;
  const restored=await origin.exchange.getSupportedTypes(),throughTarget=await target.exchange.getSupportedTypes();
  assert.deepEqual([...restored],['cfm']);assert.deepEqual([...throughTarget],['cfm']);
  assert.equal(origin.exchange.getSupportedTypes,original);assert.equal(target.exchange.getSupportedTypes,original);
  cases.push({id:'reassigned-arrow-keeps-old-code-and-origin',intermediate:[...intermediate],restored:[...restored],throughTarget:[...throughTarget],sameFunction:true});
 }
 assert.equal(cases.length,4,'independent arrow-value scenario floor');
 assert.deepEqual(cases.map(item=>item.id),['supported-arrow-retains-origin','finder-arrow-retains-origin','transplanted-arrow-reads-origin-option-slot','reassigned-arrow-keeps-old-code-and-origin'],'independent arrow-value scenario content');
 process.stdout.write(JSON.stringify({cases}));
})().catch(error=>{console.error(error);process.exit(1);});
