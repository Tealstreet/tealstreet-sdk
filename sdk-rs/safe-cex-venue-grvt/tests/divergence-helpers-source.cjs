'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'GRVT AUDIT isolated compiler');
const ts = require(compiler);
const deps = process.env.SAFE_CEX_CRYPTO_ORACLE_DEPS;
assert(deps && path.isAbsolute(deps), 'GRVT AUDIT isolated crypto dependencies');
const oracleRequire = require('node:module').createRequire(path.join(deps, 'package.json'));
const resolved = fs.realpathSync(oracleRequire.resolve('ethers'));
assert(resolved.startsWith(fs.realpathSync(deps) + path.sep), 'GRVT AUDIT ethers installation isolation');
const ethers = oracleRequire('ethers');
assert.equal(ethers.version, '6.17.0', 'GRVT AUDIT actual address validator');
const dir = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt');
function tree(name) { const file = path.join(dir, name); return ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true); }
function declarations(ast, names) {
  return names.map(name => {
    const found = ast.statements.filter(n => n.name?.text === name || (ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === name)));
    assert.equal(found.length, 1, `GRVT AUDIT unique declaration ${name}`);
    return found[0].getText(ast);
  }).join('\n');
}
function execute(source, context) {
  const compiled = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
  assert.equal(compiled.diagnostics.length, 0, 'GRVT AUDIT actual body transpile');
  vm.runInNewContext(compiled.outputText, context);
}
const builder = tree('grvt.builder.ts'), tables = tree('grvt.types.ts'), exchange = tree('grvt.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtExchange');
function member(name) { const found = cls.members.filter(n => n.name?.getText(exchange) === name); assert.equal(found.length, 1); return found[0]; }
const names = ['grvtProxy','viaGrvtProxy','getGrvtEdgeApiUrl','getGrvtTradingApiUrl','getStringField','getValidAddress','grvtBuilderFeeRateToRaw','getGrvtBuilderAddress','createGrvtBuilderInfo','GrvtBuilderUnauthorizedError','authenticateGrvt','fetchGrvtMainAccountId','authorizeGrvtBuilderWithBrowserWallet'];
const constants = ['TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET','TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET','DEFAULT_GRVT_BUILDER_FEE_RATE'];
let proxy;
const walletEffects = [];
class BrowserProvider {
  async send(method) {
    walletEffects.push(method);
    if(method === 'eth_requestAccounts') return ['0x52908400098527886E0F7030069857D2E4169EE7'];
    assert.equal(method,'wallet_switchEthereumChain','GRVT AUDIT only pre-fee wallet hooks');
  }
}
const context = {exports:{}, isAddress:ethers.isAddress, getTransport:()=>({privateProxy:()=>proxy}), LogSeverity:{Debug:'debug',Error:'error',Info:'info'},
  getEthereumProvider:()=>({}), BrowserProvider, resolveGrvtProxyHeaders:async()=>({}), AbortController, setTimeout:()=>1, clearTimeout:()=>{}};
execute(declarations(tables,['BASE_URLS','ENDPOINTS'])+'\n'+declarations(builder,[...constants,...names])+'\nObject.assign(exports,{'+names.join(',')+'});\nexports.symbol = function(symbol:string) '+member('symbolToInstrumentId').body.getText(exchange)+';\nexports.cancel = async function(symbol:string) '+member('cancelSymbolOrders').initializer.body.getText(exchange)+';\nexports.markets = async function() '+member('fetchMarkets').initializer.body.getText(exchange),context);
const api = context.exports;
function units(value) { assert.equal(typeof value,'string','GRVT AUDIT returned string type'); return Array.from({length:value.length},(_,i)=>value.charCodeAt(i)); }
const observations = [];
function observe(name, value, unit, raw) {
  const observed = units(value);
  assert(observed.includes(unit), `GRVT AUDIT retained unit ${name}`);
  observations.push({name,unit,units:observed,raw});
}
(async()=>{
  for(const unit of [0xd800,0xdc00]) {
    const text=String.fromCharCode(unit), raw=JSON.stringify(text);
    proxy={url:text};
    observe('via',api.viaGrvtProxy('https://edge.grvt.io'),unit,raw);
    let funding;
    for(const testnet of [false,true]) {
      let outgoing;
      context.fetch=async url=>{outgoing=url;return {ok:true,json:async()=>({funding_account_address:text}),headers:{get:name=>name==='x-grvt-account-id'?'account':null}};};
      const authenticated=await api.authenticateGrvt('fixture-api',testnet,{});
      assert.equal(authenticated.success,true,'GRVT AUDIT actual authentication success');
      funding=authenticated.fundingAccountAddress;
      assert(outgoing.endsWith('/auth/api_key/login'),'GRVT AUDIT actual outgoing edge endpoint');
      observe(`edge-${testnet}`,outgoing,unit,raw);
      await api.fetchGrvtMainAccountId(testnet,{},authenticated);
      assert(outgoing.endsWith('/full/v1/aggregated_account_summary'),'GRVT AUDIT actual outgoing trading endpoint');
      observe(`trading-${testnet}`,outgoing,unit,raw);
    }
    assert.equal(api.getStringField(text),funding,'GRVT AUDIT actual helper and funding return');
    observe('field',funding,unit,raw);
    const valid='0x52908400098527886E0F7030069857D2E4169EE7';
    assert.equal(api.getValidAddress(text,valid),valid,'GRVT AUDIT candidate skipped before valid address');
    observe('error',new api.GrvtBuilderUnauthorizedError({reason:'unauthorized',requiredRaw:1,message:text}).message,unit,raw);
    try { api.grvtBuilderFeeRateToRaw(text); assert.fail('GRVT AUDIT expected invalid fee'); }
    catch(error) { assert(units(error.message).includes(unit),'GRVT AUDIT caught fee-error unit'); }
    walletEffects.length=0;
    const authorization=await api.authorizeGrvtBuilderWithBrowserWallet('fixture-api',valid,text,false);
    assert.equal(authorization.success,false,'GRVT AUDIT actual authorization catches invalid fee');
    assert.deepEqual(walletEffects,['eth_requestAccounts','wallet_switchEthereumChain'],'GRVT AUDIT authorization reaches fee after wallet hooks');
    observe('fee-error',authorization.message,unit,raw);
    assert.equal(api.grvtBuilderFeeRateToRaw('0.03'+text),300,'GRVT AUDIT numeric prefix drops unit');
    observe('builder',api.createGrvtBuilderInfo(false,text,'0.03').builder,unit,raw);
    let sent;
    await api.cancel.call({grvtClient:{tdgClient:{cancelAllOrders:async value=>{sent=value;}},authenticatedEndpoint:async()=>({})},grvtTradingAccountId:'account',symbolToInstrumentId:api.symbol,log:()=>{}},text+'_PERP');
    assert.equal(sent.instrument_id.slice(1),'_Perp','GRVT AUDIT outgoing normalized suffix');
    observe('cancel-instrument',sent.instrument_id,unit,JSON.stringify(text+'_PERP'));
    const data={r:[{i:'btc_perp',b:text,q:'USD',k:'PERPETUAL'},{i:'eth_perp',b:'ETH',q:'USD'}]};
    const messages=[];
    const markets=await api.markets.call({xhr:{post:async()=>({data})},log:message=>messages.push(message)});
    assert.equal(markets.length,2,'GRVT AUDIT source whole market batch retained');
    assert.equal(markets[1].base,'ETH','GRVT AUDIT source unaffected market retained');
    assert.deepEqual(messages,['Fetched 2 GRVT markets'],'GRVT AUDIT source success log');
    observe('markets-base',markets[0].base,unit,JSON.stringify(data));
  }
  assert.equal(observations.length,22,'GRVT AUDIT completed helper observations');
  process.stdout.write(JSON.stringify({observations,completed:22}));
})().catch(error=>{console.error(error);process.exitCode=1;});
