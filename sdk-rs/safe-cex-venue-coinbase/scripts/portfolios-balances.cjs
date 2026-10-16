const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const root=path.resolve(__dirname,'../../..');
function source(name){const p=path.join(root,'packages/safe-cex/src/exchanges/coinbase/'+name);return ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true);}
const main=source('coinbase.exchange.ts'),spot=source('coinbasespot.exchange.ts'),types=source('coinbase.types.ts');
function fields(file,owner,names){const c=file.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===owner);assert(c);return names.map(name=>{const f=c.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(file)===name);assert(f?.initializer,'live '+owner+'.'+name);return name+' = '+f.initializer.getText(file)+';';}).join('\n');}
const methods=['fetchPortfolios','findSelectedPortfolio','getSupportedTypes','fetchBalance','fetchSpotCurrencies','fetchBalanceCfm','fetchBalanceIntx'];
const predicate=spot.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(spot)==='shouldRethrowPortfolioProbeError');assert(predicate);
const endpoints=types.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(types)==='ENDPOINTS'));assert(endpoints);
const text=[endpoints.getText(types),'const shouldRethrowPortfolioProbeError='+predicate.initializer.getText(spot)+';',`class Base {${fields(main,'CoinbaseExchange',methods)}}`,`class Spot extends Base {${fields(spot,'CoinbaseSpotExchange',['findSelectedPortfolio','getSupportedTypes'])}}`,'export {Base,Spot,shouldRethrowPortfolioProbeError};'].join('\n');
const exported={};vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:exported,Error});
function capture(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {$bits:b.toString('hex')};}if(require('node:util').types.isNativeError(v))return {name:v.name,message:v.message};if(v&&typeof v==='object'){if(Array.isArray(v))return v.map(capture);return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,capture(v)]));}return v;}
function setup(input){const trace=[],errors=[];const r=new (input.op==='spotFind'||input.op==='spotTypes'?exported.Spot:exported.Base)();
 r.selectedPortfolio=input.selected;r.isPublicDataOnly=input.publicOnly??false;r.supportSpot=input.supportSpot??false;
 const currencies={cached:{total:7,free:6,used:1,upnl:0}},info={retained:23};const cached={total:11,free:9,used:2,upnl:4,currencies,info,primitive:5};if(input.noCurrencies)delete cached.currencies;r.store={balance:cached,positions:(input.upnls??[]).map(unrealizedPnl=>({unrealizedPnl}))};
 let i=0;r.xhr={get(endpoint,opts){trace.push({endpoint,uuid:opts?.params?.portfolio_uuid});const plan=input.replies?.[i++];assert(plan,'independent request plan '+input.label);if(Object.hasOwn(plan,'error')){errors.push(plan.error);return Promise.reject(plan.error);}return Promise.resolve({data:{...plan.data,...(endpoint.endsWith('/portfolios')?{portfolios:input.portfolios??[]}: {})}});}};
 r.emitAndLogError=(...args)=>trace.push({emit:args});if(input.override)r.getSupportedTypes=async()=>input.override;
 return {r,trace,errors,cached,currencies,info};}
(async()=>{const fixture=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/fixtures/portfolios-balances.json'),'utf8'));assert.equal(fixture.length,47,'independent portfolio corpus floor');const out=[];
 for(const input of fixture){const {r,trace,errors,cached,currencies,info}=setup(input);let result,error,failed=false;
 try {if(input.op==='predicate')result=exported.shouldRethrowPortfolioProbeError(input.error);else result=await r[({spotFind:'findSelectedPortfolio',spotTypes:'getSupportedTypes'})[input.op]??input.op]();}catch(e){failed=true;error=e;}
 const identity={cached:result===cached,currencies:result===currencies,portfolio:(input.portfolios??[]).indexOf(result),list:result===input.portfolios,info:result?.info===info,error:errors.indexOf(error)};
 const prototype=input.op==='fetchSpotCurrencies'&&result?Object.getPrototypeOf(result):null;
 out.push({result:capture(result),error:failed?capture(error):null,trace:capture(trace),identity,selected:capture(r.selectedPortfolio),portfolios:capture(input.portfolios),prototype:prototype&&Object.hasOwn(prototype,'total')?capture(prototype):null});
 }
 // Controlled completion order: base must await every probe before choosing input index zero.
 const {r,trace}=setup({label:'deferred',portfolios:[],replies:[]});const ps=[{name:'a',uuid:'a',type:'INTX',deleted:false},{name:'b',uuid:'b',type:'INTX',deleted:false}];const resolves=[];
 r.fetchPortfolios=async()=>ps;r.xhr.get=(endpoint,opts)=>{trace.push(opts.params.portfolio_uuid);return new Promise(resolve=>resolves.push(resolve));};let settled=false;const pending=r.findSelectedPortfolio().then(p=>{settled=true;return p;});await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(trace,['a','b'],'base concurrent launch');resolves[1]({data:{}});await new Promise(resolve=>setImmediate(resolve));assert.equal(settled,false,'base wait-all');resolves[0]({data:{}});assert.equal(await pending,ps[0],'base input priority');
 const mutation=setup({label:'callback',replies:[]});const mutable=[{name:'a',uuid:'a',type:'INTX',deleted:false},{name:'b',uuid:'b',type:'INTX',deleted:false}];const calls=[];mutation.r.fetchPortfolios=async()=>mutable;
 mutation.r.xhr.get=(endpoint,opts)=>{calls.push(opts.params.portfolio_uuid);if(calls.length===1){mutable[0].uuid='mut-a';mutable[1].uuid='mut-b';mutable.push({name:'extra',uuid:'extra',type:'INTX',deleted:false});}return Promise.resolve({data:{}});};
 assert.equal(await mutation.r.findSelectedPortfolio(),mutable[0],'callback retains portfolio identity');assert.deepEqual(calls,['a','mut-b'],'map captured length and live reads');
 process.stdout.write(JSON.stringify(out));
})().catch(e=>{console.error(e);process.exit(1);});
