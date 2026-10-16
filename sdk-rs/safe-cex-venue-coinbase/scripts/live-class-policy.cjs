const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const {captureHeader}=require('./special-numbers.cjs');
const root=path.resolve(__dirname,'../../..');
function source(relative){const filename=path.join(root,'packages/safe-cex/src',relative);return ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);}
const main=source('exchanges/coinbase/coinbase.exchange.ts'),spot=source('exchanges/coinbase/coinbasespot.exchange.ts'),base=source('exchanges/base.ts'),types=source('exchanges/coinbase/coinbase.types.ts');
function klass(file,name){const result=file.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text===name);assert(result,'live class '+name);return result;}
function variable(file,name){const node=file.statements.filter(ts.isVariableStatement).flatMap(node=>[...node.declarationList.declarations]).find(node=>node.name.getText(file)===name);assert(node?.initializer,'live variable '+name);return `const ${name}=${node.initializer.getText(file)};`;}
function member(file,name,key){const node=klass(file,name).members.find(node=>node.name?.getText(file)===key);assert(node,'live member '+name+'.'+key);return node.getText(file);}
const text=[...['ENDPOINTS','GRAFANA_ENDPOINTS_EVERY','GRAFANA_ENDPOINTS_PERIODIC'].map(name=>variable(types,name)),variable(spot,'shouldRethrowPortfolioProbeError'),`class BaseExchange {
 constructor(name,options,store){this.name=name;this.options=options;this.store=store;}
 ${member(base,'BaseExchange','canSign')}
 ${member(base,'BaseExchange','isPublicDataOnly')}
 createXhrWithSecondaryKey(options,factory,config){this.captureBaseMethod?.(this.getSupportedTypes);return config.rateLimit(factory(options,this));}
 emitAndLogError(...args){this.store.errors.push(args);}
}`,klass(main,'CoinbaseExchange').getText(main),klass(spot,'CoinbaseSpotExchange').getText(spot),'export{CoinbaseExchange,CoinbaseSpotExchange};'].join('\n');
function setup({spot=false,options={},captureBase=false,deferBreakdown=false}={}){
 const trace=[],store={errors:[]},capturedOptions=[],portfolios=[{uuid:'intx',type:'INTX'},{uuid:'ordinary',type:'DEFAULT'}];
 const out={};let captured,capturedFinder,releaseBreakdown,seeBreakdown;const reachedBreakdown=new Promise(resolve=>{seeBreakdown=resolve;});
 const emitted=ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(emitted,{exports:out,patternClientOrderIdRule:()=>({}),uuid:()=> 'fixture-uuid',rateLimit:x=>x,CoinbasePublicWebsocket:class{constructor(parent){this.parent=parent;}},CoinbasePrivateWebsocket:class{constructor(parent){this.parent=parent;}},createAPI(input,parent){capturedOptions.push(input);if(captureBase&&!captured){captured=parent.getSupportedTypes;capturedFinder=parent.findSelectedPortfolio;}return {get(endpoint,config){trace.push({endpoint,uuid:config?.params?.portfolio_uuid});if(endpoint==='/api/v3/brokerage/portfolios')return Promise.resolve({data:{portfolios}});if(endpoint==='/api/v3/brokerage/portfolios/{portfolio_uuid}') {if(deferBreakdown)return new Promise(resolve=>{releaseBreakdown=resolve;seeBreakdown();});return Promise.resolve({data:{}});}if(endpoint==='/api/v3/brokerage/cfm/balance_summary')return Promise.reject({response:{status:403}});throw new Error('unreviewed live-class endpoint '+endpoint);}};}});
 const C=spot?out.CoinbaseSpotExchange:out.CoinbaseExchange;
 const exchange=new C('coinbase',options,store);
 assert.equal(capturedOptions.length,2,'independent two API roots');assert.equal(capturedOptions[0],options);assert.equal(capturedOptions[1],options);
 return {exchange,trace,store,portfolios,capturedOptions,captured,capturedFinder,reachedBreakdown,releaseBreakdown(){assert.equal(typeof releaseBreakdown,'function');releaseBreakdown({data:{}});}};
}
module.exports = {setup};
if (require.main === module) {
(async()=>{
 const cases=[];
 for(const [id,options] of [['no-credentials',{}],['key-without-signer',{key:'fixture-key'}]]){
  const c=setup({options});const types=await c.exchange.getSupportedTypes();assert.deepEqual([...types],['intx']);assert.equal(c.trace.length,0);assert.equal(c.exchange.selectedPortfolio,null);cases.push({id,types:[...types],calls:c.trace.length,selected:null});
 }
 {
  const c=setup({options:{key:'fixture-key',secret:'fixture-secret'}});c.portfolios.shift();const types=await c.exchange.getSupportedTypes();assert.deepEqual([...types],['spot']);assert.equal(c.exchange.selectedPortfolio,c.portfolios[0]);const account=await c.exchange.getAccount();assert.equal(account.userId,'ordinary');assert.equal(c.trace.length,3);cases.push({id:'local-selection-and-account',types:[...types],account:{userId:account.userId},trace:c.trace,sameSelected:true});
 }
 {
  const options={key:'fixture-key',remoteSigner:async()=>({})};const c=setup({options});c.exchange.selectedPortfolio=c.portfolios[1];const types=await c.exchange.getSupportedTypes();assert.deepEqual([...types],['spot']);assert.equal(c.trace.length,0);cases.push({id:'remote-signer',types:[...types],calls:0});
 }
 {
  const original={key:'fixture-key',secret:'fixture-secret'},replacement={};const c=setup({options:original});c.exchange.options=replacement;const types=await c.exchange.getSupportedTypes();assert.deepEqual([...types],['intx']);assert.equal(c.exchange.options,replacement);assert.equal(c.capturedOptions[0],original);assert.equal(c.capturedOptions[1],original);assert.equal(c.trace.length,0);cases.push({id:'replace-options-slot',types:[...types],calls:0,factoriesKeepOriginal:true});
 }
 {
  const c=setup({options:{key:'fixture-key',secret:'fixture-secret'}});c.exchange.selectedPortfolio={uuid:'cfm',type:'CFM'};c.exchange.supportSpot=false;const without=await c.exchange.getSupportedTypes();c.exchange.supportSpot=true;const withSpot=await c.exchange.getSupportedTypes();assert.deepEqual([...without],['cfm']);assert.deepEqual([...withSpot],['cfm','spot']);assert.equal(c.trace.length,0);cases.push({id:'live-support-spot',without:[...without],withSpot:[...withSpot],calls:0});
 }
 {
  const c=setup({spot:true,options:{},captureBase:true});assert.equal(typeof c.captured,'function');const currentField=c.exchange.getSupportedTypes;assert.notEqual(currentField,c.captured,'Spot installs a new supported arrow');assert.equal(currentField,c.exchange.getSupportedTypes,'current field read preserves function identity');const before=await c.captured();assert.deepEqual([...before],['intx']);assert.equal(c.trace.length,0);const current=await c.exchange.getSupportedTypes();assert.deepEqual([...current],['spot']);assert.equal(c.exchange.selectedPortfolio,c.portfolios[1]);assert.deepEqual(c.trace.map(item=>item.endpoint),['/api/v3/brokerage/portfolios','/api/v3/brokerage/portfolios/{portfolio_uuid}']);const account=await c.exchange.getAccount();assert.equal(account.userId,'ordinary');cases.push({id:'spot-override-and-captured-base-field',capturedTypes:[...before],currentTypes:[...current],sameCurrentField:true,distinctFromCaptured:true,trace:c.trace,sameSelected:true,account:{userId:account.userId}});
 }
 {
  const c=setup({spot:true,options:{key:'fixture-key',secret:'fixture-secret'},captureBase:true});c.portfolios[1].type='CFM';
  const retained=await c.captured();assert.deepEqual([...retained],['cfm','spot']);assert.equal(c.exchange.selectedPortfolio,c.portfolios[1]);
  const current=await c.exchange.getSupportedTypes();assert.deepEqual([...current],['spot']);
  assert.deepEqual(c.trace,[{endpoint:'/api/v3/brokerage/portfolios',uuid:undefined},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'ordinary'}],'captured base body reads current Spot finder field');
  cases.push({id:'signed-captured-base-calls-spot-finder',retained:[...retained],current:[...current],sameSelected:true,trace:c.trace});
 }
 {
  const c=setup({spot:true,options:{key:'fixture-key',secret:'fixture-secret'},captureBase:true});c.portfolios[1].type='CFM';
  assert.equal(typeof c.capturedFinder,'function');const retained=await c.capturedFinder();assert.equal(retained,c.portfolios[0]);assert.equal(retained.type,'INTX');
  const current=await c.exchange.findSelectedPortfolio();assert.equal(current,c.portfolios[1]);assert.equal(current.type,'CFM');assert.equal(c.exchange.selectedPortfolio,null,'finder returns roots without assigning selection');
  assert.deepEqual(c.trace,[{endpoint:'/api/v3/brokerage/portfolios',uuid:undefined},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'intx'},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'ordinary'},{endpoint:'/api/v3/brokerage/portfolios',uuid:undefined},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'ordinary'}],'retained base finder differs from current Spot finder');
  cases.push({id:'retained-base-finder-versus-current-spot',retainedUuid:retained.uuid,currentUuid:current.uuid,selected:null,trace:c.trace});
 }
 {
  const c=setup({options:{key:'fixture-key',secret:'fixture-secret'}});c.exchange.selectedPortfolio={uuid:'cfm',type:'CFM'};
  const pending=c.exchange.getSupportedTypes();const first=await pending;assert.deepEqual([...first],['cfm','spot']);first.push('intx');const second=await pending;
  assert.equal(first,second,'same source Promise returns the same mutable types array');assert.deepEqual([...second],['cfm','spot','intx']);
  const fresh=await c.exchange.getSupportedTypes();assert.notEqual(fresh,first,'new method invocation allocates distinct array');assert.deepEqual([...fresh],['cfm','spot']);assert.equal(c.trace.length,0);
  cases.push({id:'supported-types-promise-retains-array-root',second:[...second],fresh:[...fresh],samePromiseValue:true,distinctInvocation:true,calls:0});
 }
 {
  const c=setup({options:{key:'fixture-key',secret:'fixture-secret'}});c.exchange.selectedPortfolio={uuid:'cfm',type:'CFM'};
  const pending=c.exchange.getAccount();const first=await pending;assert.equal(first.userId,'cfm');first.userId='caller-mutated';const second=await pending;
  assert.equal(first,second,'same source Promise returns the same mutable account object');assert.equal(second.userId,'caller-mutated');
  const fresh=await c.exchange.getAccount();assert.notEqual(fresh,first,'new account invocation allocates distinct object');assert.equal(fresh.userId,'cfm');assert.equal(c.exchange.selectedPortfolio.uuid,'cfm');assert.equal(c.trace.length,0);
  cases.push({id:'account-promise-retains-object-root',secondUserId:second.userId,freshUserId:fresh.userId,samePromiseValue:true,distinctInvocation:true,selectedUuid:c.exchange.selectedPortfolio.uuid,calls:0});
 }

 for(const [id,spot] of [['public-only-promise-retains-array-root',false],['spot-promise-retains-array-root',true]]) {
  const c=setup({spot,options:{}});c.exchange.selectedPortfolio={uuid:'cfm',type:'CFM'};
  const wanted=spot?['spot']:['intx'];
  const promise=c.exchange.getSupportedTypes(),first=await promise;assert.deepEqual([...first],wanted);
  first.push('intx');const second=await promise;assert.equal(first,second,'same source Promise retains branch array root');assert.deepEqual([...second],[...wanted,'intx']);
  const fresh=await c.exchange.getSupportedTypes();assert.notEqual(first,fresh);assert.deepEqual([...fresh],wanted);assert.equal(c.trace.length,0);
  cases.push({id,second:[...second],fresh:[...fresh],samePromiseValue:true,distinctInvocation:true,calls:0});
 }
 {
  const c=setup({options:{key:'fixture-key',secret:'fixture-secret'},deferBreakdown:true});c.portfolios.shift();
  const pending=c.exchange.getSupportedTypes();await c.reachedBreakdown;
  assert.equal(c.exchange.selectedPortfolio,null);assert.equal(c.trace.length,2);
  c.exchange.options={};c.exchange.supportSpot=false;c.releaseBreakdown();
  const result=await pending;assert.deepEqual([...result],[]);assert.equal(c.exchange.selectedPortfolio,c.portfolios[0]);
  assert.deepEqual(c.store.errors.map(args=>[...args]),[['No supported portfolios found']]);
  assert.deepEqual(c.trace,[{endpoint:'/api/v3/brokerage/portfolios',uuid:undefined},{endpoint:'/api/v3/brokerage/portfolios/{portfolio_uuid}',uuid:'ordinary'},{endpoint:'/api/v3/brokerage/cfm/balance_summary',uuid:undefined}]);
  cases.push({id:'live-policy-after-pending-selection',types:[...result],sameSelected:true,selectedUuid:c.exchange.selectedPortfolio.uuid,trace:c.trace,errors:c.store.errors});
 }
 assert.deepEqual(cases.map(item=>item.id),['no-credentials','key-without-signer','local-selection-and-account','remote-signer','replace-options-slot','live-support-spot','spot-override-and-captured-base-field','signed-captured-base-calls-spot-finder','retained-base-finder-versus-current-spot','supported-types-promise-retains-array-root','account-promise-retains-object-root','public-only-promise-retains-array-root','spot-promise-retains-array-root','live-policy-after-pending-selection'],'independent live-class policy content pin');
 process.stdout.write(JSON.stringify({...captureHeader(),cases}));
})().catch(error=>{console.error(error);process.exit(1);});

}
