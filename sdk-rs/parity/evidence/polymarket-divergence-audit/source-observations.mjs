// All eighteen frozen explanations; actual source observations, no native gate.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler environment required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependencies=createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH),'oracle.cjs'));
const root=fileURLToPath(new URL('../../../../',import.meta.url));
const src=path.join(root,'packages/safe-cex/src');
const venue=path.join(src,'exchanges/polymarket');
const control=process.argv.includes('--control');
const high=JSON.parse('"\\ud800"'),low=JSON.parse('"\\udc00"');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function once(text,before,after){assert.equal(text.split(before).length-1,1,'POLY-CONTROL unique actual source selector');return text.replace(before,after);}
function declarations(file,names,owner){
  const source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  const nodes=owner?ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===owner).members:ast.statements;
  return names.map(name=>{
    const found=[...nodes].filter(n=>owner?n.name?.getText(ast)===name:(ts.isFunctionDeclaration(n)&&n.name?.text===name)||(ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name)));
    assert.equal(found.length,1,'actual declaration '+name);return found[0].getText(ast);
  }).join('\n');
}
function execute(body,bindings={}){
  const result=ts.transpileModule(body,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
  assert.equal(result.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
  const exports={};vm.runInNewContext(result.outputText,{exports,...bindings},{filename:'actual-polymarket-selection.ts'});return exports;
}
const domainFile=path.join(src,'types.ts'),domain=ts.createSourceFile(domainFile,fs.readFileSync(domainFile,'utf8'),ts.ScriptTarget.Latest,true);
const enums=execute(domain.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domain)).join('\n'));
function api(method,transform=text=>text){
  const constants=declarations(path.join(venue,'polymarket.constants.ts'),['ENDPOINTS']);
  const member=transform(declarations(path.join(venue,'polymarket.api.ts'),[method],'PolymarketAPI'));
  return new (execute(constants+'\nclass Selected {'+member+'}\nexports.Selected=Selected;').Selected)();
}
const market=()=>({condition_id:high,token_id:'token',outcome:'Yes',minimum_order_size:'1',minimum_tick_size:'0.01',active:true,closed:false});
const book=()=>({asset_id:high,bids:[{price:high,size:'1'}],asks:[{price:'0.9',size:'2'}]});
const price=()=>({token_id:high,price:'0.5'});
const cases={};
for(const [id,method,verb,reply,args,field] of [
  ['getMarkets','getMarkets','get',()=>[market()],[],value=>value[0].condition_id],
  ['getOrderBook','getOrderBook','get',book,['token'],value=>value.asset_id],
  ['getOrderBooks','getOrderBooks','post',()=>[book()],[['token']],value=>value[0].asset_id],
  ['getPrices','getPrices','get',()=>[price()],[['token']],value=>value[0].token_id],
  ['getServerTime','getServerTime','get',()=>({unchecked:'source returns this object'}),[],null],
])cases[id]=async()=>{
  const receiver=api(method,text=>control?once(text,'const { data } = await this.xhr.','await Promise.resolve(); const { data } = await this.xhr.'):text);
  const data=reply(),calls=[];receiver.xhr={[verb]:(...params)=>{calls.push(params);return Promise.resolve({data});}};
  const pending=receiver[method](...args);pending.catch(()=>{});
  assert.equal(calls.length,1,'POLY-PREFIX-'+id+' callback before caller awaits');
  const value=await pending;
  assert.strictEqual(value,data,'POLY-ALIAS-'+id+' source return identity');
  if(field)assert.deepEqual(units(field(value)),[0xd800],'POLY-UNIT-'+id+' returned field');
  return {id,callCount:calls.length,returnedUnit:field?[0xd800]:null,scope:'source prefix and returned value; native async body inspected only'};
};
const utilityNames=['SUPPORTED_TICK_SIZES','normalizeTickSize','parseGammaMarketTokens','precisionFromTickSize'];
function utilityProgram(transform=text=>text){return transform(declarations(path.join(venue,'polymarket.exchange.ts'),utilityNames));}
cases.normalizeTickSize=async()=>{
  const utility=utilityProgram(text=>control?once(text,"tickSize || 'missing'","'replacement'"):text);
  const method=declarations(path.join(venue,'polymarket.exchange.ts'),['placeOrder'],'PolymarketExchange');
  // Utility and public caller share one realm, preserving the source instanceof Error.
  const receiver=new (execute(utility+'\nclass Selected {'+method+'}\nexports.Selected=Selected;',enums).Selected)();
  receiver.resolveCallerClientOrderId=()=>{};receiver.wallet={};receiver.credentials={};
  receiver.symbolToToken=new Map([['fixture-symbol','fixture-token']]);
  receiver.store={marketsBySymbol:{'fixture-symbol':{info:{tickSize:high}}}};
  const emitted=[];receiver.emitAndLogError=message=>emitted.push(message);
  let error;try{await receiver.placeOrder({type:enums.OrderType.Limit,side:enums.OrderSide.Buy,price:0.5,amount:1,symbol:'fixture-symbol'});}catch(e){error=e;}
  assert.equal(error?.name,'Error','POLY-TICK-ERROR class');
  assert.equal(error.message.charCodeAt('Unsupported Polymarket tick size: '.length),0xd800,'POLY-TICK-ERROR returned rejection unit');
  assert.equal(emitted.length,1,'POLY-TICK-ERROR one report');assert.equal(emitted[0],error.message,'POLY-TICK-ERROR same emitted/public error message');
  return {id:'normalizeTickSize',units:units(error.message),scope:'actual public placeOrder rejection; preflight capabilities only, no wallet/signing/HTTP assembly'};
};
cases.parseGammaMarketTokens=()=>{
  const code=utilityProgram(text=>control?once(text,'[{ tokenId, outcome }]','[{ tokenId: tokenId.charCodeAt(0) === 0xd800 ? "replacement" : tokenId, outcome }]'):text);
  const fn=execute(code+'\nexports.parseGammaMarketTokens=parseGammaMarketTokens;').parseGammaMarketTokens;
  const value=fn({clobTokenIds:'["ordinary","\\ud800","third"]',outcomes:'["Yes","No","\\udc00"]'});
  assert.equal(value.length,3,'POLY-GAMMA fixed three paired outputs');assert.equal(value[0].tokenId,'ordinary','POLY-GAMMA ordinary pair retained');
  assert.deepEqual(units(value[1].tokenId),[0xd800],'POLY-GAMMA token unit');assert.deepEqual(units(value[2].outcome),[0xdc00],'POLY-GAMMA outcome unit');
  return {id:'parseGammaMarketTokens',count:3,tokenUnits:[0xd800],outcomeUnits:[0xdc00]};
};
cases.precisionFromTickSize=()=>{
  const code=utilityProgram(text=>control?once(text,'if (!Number.isFinite(tick) || tick <= 0) return 2;','if (!Number.isFinite(tick) || tick <= 0) return 5;'):text);
  const fn=execute(code+'\nexports.precisionFromTickSize=precisionFromTickSize;').precisionFromTickSize;
  assert.equal(fn(high),2,'POLY-PRECISION numeric fallback');assert.equal(fn(low),2,'POLY-PRECISION low-unit fallback');
  return {id:'precisionFromTickSize',result:2,scope:'numeric output erases unit; no forced retained-output inference'};
};
for(const [name,method,reply,args,extract,replacement] of [
  ['PolymarketBalanceAllowance','getBalanceAllowance',()=>({balance:high,allowance:'1'}),[0],value=>value.balance,"return {...data,balance:'replacement'};"],
  ['PolymarketClobMarket','getMarkets',()=>[market()],[],value=>value[0].condition_id,"return data.map(row=>({...row,condition_id:'replacement'}));"],
  ['PolymarketCredentials','deriveApiKey',()=>({apiKey:high,secret:'synthetic-unused',passphrase:'synthetic-unused'}),['address',1,'signature',2],value=>value.apiKey,"return {...data,apiKey:'replacement'};"],
  ['PolymarketDeriveKeyResponse','deriveApiKey',()=>({apiKey:high,secret:'synthetic-unused',passphrase:'synthetic-unused'}),['address',1,'signature',2],value=>value.apiKey,"return {...data,apiKey:'replacement'};"],
  ['PolymarketOrderBook','getOrderBook',book,['token'],value=>value.asset_id,"return {...data,asset_id:'replacement'};"],
  ['PolymarketOrderBookLevel','getOrderBook',book,['token'],value=>value.bids[0].price,"return {...data,bids:data.bids.map(row=>({...row,price:'replacement'}))};"],
  ['PolymarketOrderResponse','postOrders',()=>[{success:true,orderID:high,status:'matched'}],[[]],value=>value[0].orderID,"return data.map(row=>({...row,orderID:'replacement'}));"],
  ['PolymarketPrice','getPrices',()=>[price()],[['token']],value=>value[0].token_id,"return data.map(row=>({...row,token_id:'replacement'}));"],
])cases[name]=async()=>{
  const receiver=api(method,text=>control?once(text,'return data;',replacement):text);
  receiver.getCredentials=()=>({apiKey:'synthetic-unused'});
  const data=reply();receiver.xhr={get:async()=>({data}),post:async()=>({data})};
  const value=await receiver[method](...args);
  assert.deepEqual(units(extract(value)),[0xd800],'POLY-RECORD-'+name+' materialized unit');
  return {id:name,producer:method,units:[0xd800],scope:'actual returned declared-shape field; native record refusal inspected only'};
};
cases.PolymarketDeriveKeyRequest=async()=>{
  const receiver=api('deriveApiKey',text=>control?once(text,'POLY_ADDRESS: address',"POLY_ADDRESS: 'replacement'"):text);
  let recorded;receiver.xhr={get:async(url,config)=>{recorded=config.headers;return {data:{}};}};
  await receiver.deriveApiKey(high,1,'synthetic-unused',2);
  assert.deepEqual(units(recorded.POLY_ADDRESS),[0xd800],'POLY-RECORD-PolymarketDeriveKeyRequest materialized unit');
  return {id:'PolymarketDeriveKeyRequest',units:[0xd800],scope:'actual method header passed to recording callback, no live acceptance claim'};
};
cases.PolymarketL2Headers=()=>{
  let code=declarations(path.join(venue,'polymarket.signing.ts'),['generateHmacSignature','buildL2Headers']);
  if(control)code=once(code,"'POLY_ADDRESS': walletAddress","'POLY_ADDRESS': 'replacement'");
  const fn=execute(code+'\nexports.buildL2Headers=buildL2Headers;',{ethers:dependencies('ethers')}).buildL2Headers;
  // Public all-zero test bytes, never a wallet/API credential; no secret output.
  const value=fn({apiKey:'synthetic-unused',secret:Buffer.alloc(32).toString('base64'),passphrase:'synthetic-unused'},high,'GET','/fixture',undefined,2);
  assert.deepEqual(units(value.POLY_ADDRESS),[0xd800],'POLY-RECORD-PolymarketL2Headers materialized unit');
  return {id:'PolymarketL2Headers',units:[0xd800],scope:'actual helper and HMAC library with public test bytes; no authenticated request'};
};
assert.equal(Object.keys(cases).length,18,'POLY-AUDIT independent eighteen-profile floor');
const profile=process.argv[2],observations=[];
if(profile){assert.ok(Object.hasOwn(cases,profile),'named profile');observations.push(await cases[profile]());}
else{assert.equal(control,false,'control requires profile');for(const fn of Object.values(cases))observations.push(await fn());}
// Materialized units/caller effects are asserted before output serialization.
process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,observations})+'\n');
