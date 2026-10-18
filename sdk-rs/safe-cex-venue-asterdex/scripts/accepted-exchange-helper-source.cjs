// Fixed expectations checked against the maintained source, not Rust output.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const scripts = __dirname;
const { ts, root } = require(path.join(scripts, 'source.cjs'));
const printer = ts.createPrinter();
const ast = relative => ts.createSourceFile(relative, fs.readFileSync(path.join(root, relative), 'utf8'), ts.ScriptTarget.Latest, true);
function compile(source, bindings = {}) {
  const exports = {};
  new Function('exports', ...Object.keys(bindings), ts.transpileModule(source, { compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS} }).outputText)(exports, ...Object.values(bindings));
  return exports;
}
const shared = ast('packages/safe-cex/src/types.ts');
const enums = shared.statements.filter(ts.isEnumDeclaration).map(n => printer.printNode(ts.EmitHint.Unspecified, n, shared)).join('\n');
const ids = ast('packages/safe-cex/src/utils/createPositionId.ts');
const idSource = ids.statements.filter(n => !ts.isImportDeclaration(n)).map(n => printer.printNode(ts.EmitHint.Unspecified, n, ids)).join('\n');
const exchange = ast('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const cls = exchange.statements.find(ts.isClassDeclaration);
const selected = ['isAlgoOrder', 'getOpenAlgoOrdersCount', 'getMaxAlgoOrdersLimit', 'assertAlgoOrderCapacity', 'fetchLeverageBrackets', 'transformTicker', 'transformPosition', 'transformMarket', 'extractFilterValue', 'afterMethodCalledInWorker', 'assertV3Feature', 'fetchOpenInterestMap', 'collectOpenInterestFromPayload', 'fetchPublicTrades'];
const members = selected.map(name => {
  const member = cls.members.find(node => node.name?.getText(exchange) === name);
  assert.ok(member, name);
  return printer.printNode(ts.EmitHint.Unspecified, member, exchange);
});
const precision = compile(fs.readFileSync(path.join(root, 'packages/safe-cex/src/utils/ccxt_helpers/precision.ts'), 'utf8'));
const { Oracle } = compile(`${enums}\n${idSource}\nexport class Oracle { ${members.join('\n')} }`, {precisionToString: precision.precisionToString, LogSeverity: {Info:'info', Warning:'warning', Error:'error'}});
const load = require('./load-module.cjs');
const failure = load('packages/safe-cex/src/utils/httpFailure.ts', [], {});
const context = load('packages/safe-cex/src/utils/httpFailureContext.ts', [], {'./httpFailure':failure});
const baseAst = ast('packages/safe-cex/src/exchanges/base.ts');
const baseClass = baseAst.statements.find(node => ts.isClassDeclaration(node) && node.name.text==='BaseExchange');
assert.ok(baseClass);
const emitMethod = baseClass.members.find(node => node.name?.getText(baseAst)==='emitAndLogError');
assert.ok(emitMethod);
const { BaseOracle } = compile(`export class BaseOracle { ${printer.printNode(ts.EmitHint.Unspecified,emitMethod,baseAst)} }`, {
  readHttpFailureContext:context.readHttpFailureContext,
  reportException:()=>{},
  formatErrorForSentry:()=>{throw Error('unexpected enhanced error in string guard fixture');},
  console:{error:()=>{},trace:()=>{},warn:()=>{}},
});
const ex = new Oracle();
const bits = value => { const bytes=Buffer.alloc(8); bytes.writeDoubleBE(value); return bytes.toString('hex'); };
const result = action => { try { action(); return {ok:true}; } catch(error) { return {ok:false,name:error.name,message:error.message}; } };
ex.getFapiPath = name => `/${name}`;
ex.log = () => {};
const adapters = {
  is_raw_algo_order: input => ex.isAlgoOrder({type:'limit',info:input.info}),
  get_open_algo_orders_count: input => { ex.store={orders:input.orders}; return ex.getOpenAlgoOrdersCount(input.symbol); },
  assert_algo_order_capacity: input => { ex.store={orders:input.orders,marketsBySymbol:{BTC:{info:input.info}}}; return result(()=>ex.assertAlgoOrderCapacity('BTC',input.added)); },
  parse_leverage_brackets_response: async input => {
    let log;
    ex.primaryAPI={get:async()=>{if(input.error)throw Error(input.error);return {data:input.data};}};
    ex.log=(message,severity)=>{log={message,warning:severity==='warning'};};
    const map=await ex.fetchLeverageBrackets();
    return {entries:[...map].map(([key,value])=>({keyPresent:key!==undefined,key:key??null,valuePresent:value!==undefined,value:value??null})),...log};
  },
  transform_ticker: input => {
    const ticker=ex.transformTicker(input.ticker,input.oi===undefined?undefined:new Map([['BTC',input.oi]]));
    return {symbol:ticker.symbol,last:bits(ticker.last),percentage:bits(ticker.percentage),volume:bits(ticker.volume),quoteVolume:bits(ticker.quoteVolume),openInterest:bits(ticker.openInterest)};
  },
  transform_position: input => {
    ex.store={positions:input.stored?[{id:input.stored.id,realizedPnl:input.stored.pnl}]:[]};
    const p=ex.transformPosition(input.position);
    return {ok:true,id:p.id,side:p.side,contracts:bits(p.contracts),leverage:bits(p.leverage),realizedPnl:bits(p.realizedPnl),notional:bits(p.notional)};
  },
  transform_market: input => {
    try {
      const market=ex.transformMarket(input.market,input.leverage===undefined?new Map():new Map([['BTC',input.leverage]]));
      // Approved shared numeric-limit policy applies Number at the typed field.
      return {ok:true,symbol:market.symbol,active:market.active,price:bits(market.precision.price),amount:bits(market.precision.amount),min:bits(market.limits.amount.min),max:bits(market.limits.amount.max),leverage:bits(Number(market.limits.leverage.max))};
    } catch(error) {return {ok:false,name:error.name,message:error.message};}
  },
  after_method_called_in_worker: input => {
    const effects=[];
    ex.store={setSetting:(key,value)=>{effects.push([key,value]);if(input.reject)throw Error('setting failed');}};
    try {ex.afterMethodCalledInWorker(undefined,input.method,input.arg);return {ok:true,effects,error:null};}
    catch(error){return {ok:false,effects,error:error.message};}
  },
  assert_v3_feature: input => {
    ex.apiVersion=input.version;
    const events=[];
    ex.options={};ex.isDisposed=false;ex.name='asterdex';
    ex.emitter={emit:(event,payload)=>{assert.equal(event,'error');events.push(payload);}};
    ex.emitAndLogError=BaseOracle.prototype.emitAndLogError;
    try {ex.assertV3Feature(input.feature);return {ok:true,message:null,events};}
    catch(error){return {ok:false,message:error.message,events};}
  },
  fetch_open_interest_map: async input => {
    const responses=[...input.responses],requests=[],warnings=[];
    ex.log=message=>warnings.push(message);
    ex.publicAPI={get:async(url,config)=>{requests.push({url,symbol:config.params?.symbol??null,timeout:config.timeout});const response=responses.shift();assert.ok(response);if(response.error)throw Error(response.error);return {data:response.data};}};
    const map=await ex.fetchOpenInterestMap({symbols:input.symbols,allowPerSymbolFallback:input.allow});
    return {entries:[...map].map(([key,value])=>[key,bits(value)]),warnings,requests};
  },
};
async function main(){
  assert.equal(process.argv.length,2,'exchange helper source gate takes no arguments');
  const contracts=(await import(path.join(root,'sdk-rs/safe-cex-parity/scripts/accepted-contract-files.mjs'))).loadContracts();
  let count=0,sourceBoundary=0,nativeOnly=0;
  for(const [owner,contract]of Object.entries(contracts)){
    if(!owner.startsWith('safe_cex_venue_asterdex::exchange_helpers::'))continue;
    const name=owner.split('::').at(-1);
    if(name==='retained_info'){nativeOnly+=contract.cases.length;continue;}
    if(name==='map_public_trade'){
      for(const row of contract.cases){ex.log=()=>{};ex.publicAPI={get:async()=>({data:[row.input.trade]})};const [trade]=await ex.fetchPublicTrades(row.input.symbol);assert.equal(row.output.name,'RepresentationError');assert.ok(row.output.message.includes(`side ${trade.side} `));sourceBoundary++;}
      continue;
    }
    if(!adapters[name])continue;
    for(const row of contract.cases){
      if(row.output?.name==='RepresentationError'){
        assert.equal(name,'transform_position');assert.notEqual(typeof ex.transformPosition(row.input.position).symbol,'string');sourceBoundary++;continue;
      }
      assert.deepEqual(await adapters[name](structuredClone(row.input)),row.output,`${owner}: actual TS ${JSON.stringify(row.input)}`);count++;
    }
  }
  assert.equal(count,41);assert.equal(sourceBoundary,3);assert.equal(nativeOnly,4);
  console.log('Aster exchange-helper source: 41 exact cases +3 documented shared-output boundary cases PASS; 4 Rust-only pointer cases use the native runner.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
