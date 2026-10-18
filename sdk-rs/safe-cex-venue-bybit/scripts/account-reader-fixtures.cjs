const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const dir=path.join(root,'packages/safe-cex/src/exchanges/bybit');
const typesFile=path.join(dir,'bybit.types.ts'),typesAst=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=typesAst.statements.flatMap(s=>ts.isVariableStatement(s)?Array.from(s.declarationList.declarations):[]).filter(d=>d.name.getText(typesAst)==='ENDPOINTS');
assert.equal(endpoints.length,1,'READER actual endpoint declaration');
const typeContext=vm.createContext({});
vm.runInContext(ts.transpileModule(`const ${endpoints[0].getText(typesAst)};globalThis.ENDPOINTS=ENDPOINTS;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,typeContext);
const enumsFile=path.join(root,'packages/safe-cex/src/types.ts'),enumsAst=ts.createSourceFile(enumsFile,fs.readFileSync(enumsFile,'utf8'),ts.ScriptTarget.Latest,true);
const margins=enumsAst.statements.filter(s=>ts.isEnumDeclaration(s)&&s.name.text==='MarginType');assert.equal(margins.length,1);
vm.runInContext(ts.transpileModule(`${margins[0].getText(enumsAst)};globalThis.marginEnum=MarginType;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,vm.createContext({exports:typeContext}));
function subject(family,names){
 const [file,name]=({linear:['bybit.exchange.ts','BybitExchange'],inverse:['bybitinverse.exchange.ts','BybitInverseExchange'],spot:['bybitspot.exchange.ts','BybitSpotExchange'],hyro:['hyrotrader.exchange.ts','HyrotraderExchange']})[family];
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(dir,file),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const body=names.map(n=>{const members=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(members.length,1,`${name}.${n}`);return members[0].getText(ast);}).join('\n');
 const context=vm.createContext({ENDPOINTS:typeContext.ENDPOINTS,MarginType:typeContext.MarginType});
 vm.runInContext(ts.transpileModule(`class Subject {${body}} globalThis.subject=new Subject();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);return context.subject;
}
function observe(value){
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':bytes.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const error=(name,message)=>Object.assign(new Error(message),{name});
const outcome=(e,remote,logging,writer)=>({error:e.name,message:e.message,sameRemote:e===remote,sameLogging:e===logging,sameWriter:e===writer});
const raw=value=>JSON.stringify(value);
async function main(){
 await require('./eager-sdk-fixtures.cjs')();
 const accounts=[],margins=[],balances=[],hyro=[];
 const accountInputs=[
  {data:{retCode:0,result:{userID:'USER-37',inviterID:'AFFILIATE-91'}}},
  {data:{retCode:0,result:{userID:null,inviterID:91}}},
  {data:{retCode:0,result:{userID:[3,7],inviterID:{tag:'affiliate'}}}},
  {data:{retCode:0}},{data:{retCode:0,result:null}},
  {data:{retCode:'0',retMsg:'string zero'}},{data:{retCode:false,retMsg:'boolean zero'}},
  {data:{retMsg:'missing code'}},{data:{retCode:1,retMsg:null}},{data:{retCode:1}},
  {data:{retCode:1,retMsg:['denied',7]}},{data:{retCode:0},remoteFail:true},
 ];assert.equal(accountInputs.length,12,'READER independent account input floor');
 for(const family of ['linear','inverse','spot'])for(const input of accountInputs){
  const api=subject(family,['getAccount']),events=[],remote=error('RemoteError','transport failed');
  api.xhr={get:async endpoint=>{events.push(['get',endpoint]);await Promise.resolve();events.push(['resolved']);if(input.remoteFail)throw remote;return {data:input.data};}};
  let result;try{result={value:observe(await api.getAccount())};}catch(e){result=outcome(e,remote);}
  accounts.push({family,dataRaw:raw(input.data),remoteFail:input.remoteFail||false,result,events});
 }assert.equal(accounts.length,36,'READER independent account floor');
 const marginInputs=[
  {data:{retMsg:'OK',result:{marginMode:'ISOLATED_MARGIN',unifiedMarginStatus:6}}},
  {data:{retMsg:'OK',result:{marginMode:'REGULAR_MARGIN',unifiedMarginStatus:1}}},
  {data:{retMsg:'OK',result:{marginMode:'isolated_margin',unifiedMarginStatus:'6'}}},
  {data:{retMsg:'OK'}},{data:{retMsg:'OK',result:null}},
  {data:{retCode:0,retMsg:'denied'}},{data:{retMsg:null}},{data:{}},
  {data:{retMsg:'denied'},logFail:true},
  {data:{retMsg:'OK'},remoteFail:true},
  {data:{retMsg:'OK',result:{marginMode:'ISOLATED_MARGIN',unifiedMarginStatus:6}},writeFail:'margin'},
  {data:{retMsg:'OK',result:{marginMode:'ISOLATED_MARGIN',unifiedMarginStatus:6}},writeFail:'status'},
 ];assert.equal(marginInputs.length,12,'READER independent margin input floor');
 for(const family of ['linear','inverse','spot'])for(const input of marginInputs){
  const api=subject(family,['fetchMarginAccountInfos']),events=[],remote=error('RemoteError','transport failed'),logging=error('LogError','logging failed'),writer=error('WriteError','write failed');
  let margin='cross',status=3;
  Object.defineProperty(api,'marginType',{get:()=>margin,set:value=>{events.push(['margin',observe(value)]);if(input.writeFail==='margin')throw writer;margin=value;}});
  Object.defineProperty(api,'unifiedMarginStatus',{get:()=>status,set:value=>{events.push(['status',observe(value)]);if(input.writeFail==='status')throw writer;status=value;}});
  api.xhr={get:async endpoint=>{events.push(['get',endpoint]);await Promise.resolve();events.push(['resolved']);if(input.remoteFail)throw remote;return {data:input.data};}};
  api.emitAndLogError=value=>{events.push(['log',observe(value)]);if(input.logFail)throw logging;};
  let result;try{await api.fetchMarginAccountInfos();result={ok:true};}catch(e){result=outcome(e,remote,logging,writer);}
  margins.push({family,dataRaw:raw(input.data),remoteFail:input.remoteFail||false,logFail:input.logFail||false,writeFail:input.writeFail||'',result,margin,status:observe(status),events});
 }assert.equal(margins.length,36,'READER independent margin floor');
 const balanceInputs=[
  {disposed:false,after:false},{disposed:false,after:true},{disposed:true,after:false},{disposed:true,after:true},
  {disposed:false,after:false,rawFail:true},{disposed:false,after:true,rawFail:true},
  {disposed:false,after:false,formatFail:true},{disposed:false,after:true,formatFail:true},
 ];assert.equal(balanceInputs.length,8,'READER independent balance input floor');
 for(const family of ['linear','inverse','spot'])for(const input of balanceInputs){
  const api=subject(family,['fetchBalance']),events=[],remote=error('RemoteError','raw balance failed'),writer=error('FormatError','format failed');
  const rawBalance=[{marker:'raw-37'}],formatted={total:91,marker:'formatted'},initial={total:3,marker:'initial'},stored={total:7,marker:'stored-after-await'};
  let disposed=input.disposed,current=initial;
  Object.defineProperty(api,'isDisposed',{get:()=>{events.push(['disposed',disposed]);return disposed;}});
  api.store={get balance(){events.push(['stored']);return current;}};
  api.fetchRawBalance=async()=>{events.push(['raw']);await Promise.resolve();disposed=input.after;current=stored;events.push(['resolved']);if(input.rawFail)throw remote;return rawBalance;};
  api.formatBalance=received=>{assert.equal(received,rawBalance,'READER source raw identity');events.push(['format',observe(received)]);if(input.formatFail)throw writer;return formatted;};
  let result;try{const value=await api.fetchBalance();result={value:observe(value),sameStored:value===stored,sameFormatted:value===formatted};}catch(e){result=outcome(e,remote,undefined,writer);}
  balances.push({family,input,result,events});
 }assert.equal(balances.length,24,'READER independent balance floor');
 for(const input of [{disposed:false,after:false},{disposed:false,after:true},{disposed:true,after:false},{disposed:true,after:true},{disposed:false,after:false,fetchFail:true},{disposed:false,after:true,fetchFail:true},{disposed:false,after:false,updateFail:true},{disposed:false,after:true,updateFail:true}]){
  const api=subject('hyro',['fetchMarginAccountInfos']),events=[],remote=error('RemoteError','balance failed'),writer=error('UpdateError','update failed'),balance={total:91,marker:'hyro-balance'};
  let disposed=input.disposed,updated;
  Object.defineProperty(api,'isDisposed',{get:()=>{events.push(['disposed',disposed]);return disposed;}});
  api.fetchBalance=async()=>{events.push(['balance']);await Promise.resolve();disposed=input.after;events.push(['resolved']);if(input.fetchFail)throw remote;return balance;};
  api.store={update(value){assert.deepEqual(Object.keys(value),['balance'],'READER source update keys');assert.equal(value.balance,balance,'READER source update balance identity');events.push(['update',observe(value.balance)]);if(input.updateFail)throw writer;updated=value.balance;}};
  let result;try{await api.fetchMarginAccountInfos();result={ok:true};}catch(e){result=outcome(e,remote,undefined,writer);}
  hyro.push({input,result,updated:observe(updated),sameBalance:updated===balance,events});
 }assert.equal(hyro.length,8,'READER independent Hyro floor');
 process.stdout.write(JSON.stringify({accounts,margins,balances,hyro}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
