// The source submission receiver and utility execute together without JSON error capture.
if (process.argv.includes('--unknown-outcome')) {
  const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
  const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
  const root = path.resolve(__dirname, '../../..');
  const read = file => fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
  const ast = ts.createSourceFile('source.ts', read('exchanges/bybit/bybit.exchange.ts'), 99, true);
  const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
  const method = cls.members.find(n => n.name?.getText(ast) === 'submitCreateOrders');
  assert(method, 'actual submission receiver');
  const code = ts.transpileModule(read('utils/unknownSendOutcome.ts').replace(/export /g, '') +
    '\nclass Subject { ' + method.getText(ast).replace(/^protected /, '') + ' }',
    {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  const {mapSeries} = require('p-iteration');
  const {Subject,isUnknownOutcome} = new Function('mapSeries', 'ENDPOINTS', code + ';return {Subject,isUnknownOutcome};')(mapSeries, {CREATE_ORDER:'/v5/order/create'});
  (async () => {
    const outputs = [];
    for (let profile = 0; profile < 12; profile++) {
      const error = {message:'timeout'};
      if (profile === 0) error.opaque = {text:'\ud800', number:NaN, missing:undefined};
      if (profile === 1) Object.defineProperty(error, 'message', {value:'timeout', enumerable:false});
      if (profile === 2) {error.message='rejected'; Object.setPrototypeOf(error, {code:'ECONNRESET'});}
      if (profile === 3) error.response = {status:Infinity};
      if (profile === 4) {error.message='Remote signer unavailable'; error.response={status:503};}
      if (profile === 5) error.response = {status:400};
      if (profile === 6) {error.message='rejected'; error.response={status:400,data:{code:-1007}};}
      if (profile === 7) {error.message='rejected'; error.name='TradeTunnel';}
      if (profile === 8) Object.defineProperty(error, 'unused', {enumerable:true,get(){throw Error('unread getter');}});
      if (profile === 9) {error.message='rejected';error.name='TradeTunnel\ud800';}
      if (profile === 10) error.response={status:503,get unused(){throw Error('unread response field');}};
      const target = new Subject(); let sends=0, reports=0;
      target.xhr={post:async()=>{sends++;throw error;}};
      target.formatBybitVenueError=()=> 'reported'; target.emitAndLogError=()=>{reports++;};
      let rejected=false, original=false;
      try {const result=await target.submitCreateOrders([{}],{clientOrderId:profile===11?undefined:'caller'});assert.deepEqual(result,[]);}
      catch (e) {rejected=true;original=e===error;}
      assert.equal(sends,1);assert.equal(reports,1);
      outputs.push({rejected,original,sends,reports});
    }
    for (let profile=12;profile<28;profile++) {
      const events=[], marker={marker:73}, error={message:'timeout'};
      if(profile===12) for(const [key,value] of Object.entries({code:undefined,message:'timeout',name:undefined,response:undefined})) Object.defineProperty(error,key,{configurable:true,get(){events.push(key);return value;}});
      if(profile===13) Object.defineProperty(error,'name',{get(){throw marker;}});
      if(profile===14) error.message={toString(){events.push('string');return 'timeout';}};
      if(profile===15) error.message={toString(){throw marker;}};
      if(profile===16) error.response={status:503,get data(){throw marker;}};
      if(profile===17) error.response={status:400,get data(){throw marker;}};
      let input=error;
      if(profile===18) {input=function(){};input.message='timeout';}
      if(profile===19) {input=[];Object.setPrototypeOf(input,{message:'timeout'});}
      if(profile===20) error.message=null;
      if(profile===21) error.response={status:{valueOf(){events.push('status');return 400;}},data:{code:{valueOf(){events.push('venue');return -1007;}}}};
      if(profile===22) error.response={status:503n};
      if(profile===23) error.response={status:400,data:{code:-1007n}};
      if(profile===24) error.response={status:{valueOf(){events.push('big-status');return 503n;}}};
      if(profile===25) error.response={status:499n,data:{code:{valueOf(){events.push('big-venue');return -1007n;}}}};
      if(profile===26) error.response={status:10n**400n};
      if(profile===27) error.response={status:499n,data:{code:10n**400n}};
      let value=null,original=false;try{value=isUnknownOutcome(input);}catch(e){assert.equal(e,marker);original=true;}
      outputs.push({value,original,events});
    }
    assert.equal(outputs.length,28);process.stdout.write(JSON.stringify(outputs));
  })().catch(e=>{console.error(e);process.exitCode=1;});
} else {
// Maintained class bodies run together; scripted I/O supplies the same envelopes
// as the native physical transport. No batch or request-format policy is copied.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = file => fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
const compilerOptions = {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS};
function moduleFrom(file, dependencies = {}) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(file), {compilerOptions}).outputText, {
    exports, require(name) {
      assert(Object.hasOwn(dependencies, name), `${file}: ${name}`);
      return dependencies[name];
    },
  });
  return exports;
}
const ast = file => ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true);
const variables = tree => tree.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]);
const exchange = ast('exchanges/bybit/bybit.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
function member(name) {
  const nodes = cls.members.filter(n => n.name?.getText(exchange) === name);
  assert.equal(nodes.length, 1, name);
  return nodes[0].getText(exchange);
}
function declaration(tree, name) {
  const node = variables(tree).find(n => n.name.getText(tree) === name);
  assert(node, name);
  return `const ${node.getText(tree)};`;
}
const typeTree = ast('types.ts');
const enumSource = typeTree.statements.filter(ts.isEnumDeclaration).map(n => n.getText(typeTree).replace(/^export\s+/, '')).join('\n');
const tables = ast('exchanges/bybit/bybit.types.ts');
const source = enumSource + '\n' + [
  'BYBIT_TRADING_AGREEMENT_RET_CODES', 'numberOrUndefined', 'stringFromUnknown',
  'getBybitErrorField', 'formatBybitTradingAgreementError', 'getBybitPayloadMessage',
  'getBybitTriggerBy', 'getBybitTriggerDirectionPrice',
].map(name => declaration(exchange, name)).join('\n') + '\n' + [
  'ENDPOINTS', 'ORDER_SIDE', 'ORDER_TYPE', 'ORDER_TIME_IN_FORCE_OUTBOUND',
].map(name => declaration(tables, name)).join('\n') + '\n' +
  `globalThis.Subject = class {${[
    'placeOrders', 'placeBatchOrders', 'placeBatchOrdersForCategory', 'processBatch',
    'formatOrderForBatch', 'getOrderExecutionParams', 'getOrderPositionIdx',
    'getCategoryForSymbol', 'formatBybitVenueError',
  ].map(member).join('\n')}};`;
const enumContext = vm.createContext({});
vm.runInContext(ts.transpileModule(enumSource, {compilerOptions}).outputText, enumContext);
const enums = vm.runInContext('({OrderType,OrderTimeInForce})', enumContext);
const params = moduleFrom('exchanges/bybit/bybit.order-params.ts', {'../../types':enums});
const inverse = moduleFrom('exchanges/bybit/bybit.inverse.ts');
const options = moduleFrom('exchanges/bybit/bybit.options.ts');
const {inverseObj} = moduleFrom('utils/inverseObj.ts');
const {omitUndefined} = moduleFrom('utils/omitUndefined.ts', {'lodash/omitBy':{__esModule:true,default:require('lodash/omitBy')}});
const {adjust} = moduleFrom('utils/safe-math.ts', {'@tealstreet/math/src/isNotNumber': moduleFrom('../../math/src/isNotNumber.ts')});
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
assert.equal(cases.length, 24, 'independent source batch case floor');
(async () => {
  const outputs = [];
  for (const fixture of cases) {
    const requests = [], errors = [], sleeps = [], recorded = [];
    const context = vm.createContext({
      ...params, ...inverse, ...options, inverseObj, omitUndefined, adjust,
      chunk:require('lodash/chunk'), sleep:async ms => {sleeps.push(ms)},
    });
    vm.runInContext(ts.transpileModule(source, {compilerOptions}).outputText, context);
    const target = new context.Subject();
    target.name = 'bybit';
    target.options = {enableInverseMarkets:true};
    target.batchLimit = 10;
    target.store = {
      marketsBySymbol:Object.fromEntries(fixture.markets.map(m => [m.symbol, m])),
      tickersBySymbol:Object.fromEntries(fixture.markets.map(m => [m.symbol, {last:100,mark:101,index:102}])),
    };
    target.assertCallerClientOrderIds = () => {};
    target._hasFullSizedPositionOppositeSide = () => false;
    target.fetchPositionMode = async () => fixture.mode;
    target.assertOptionOrderPlaceable = () => {throw Error('unexpected option in regular batch corpus')};
    target.placeOrder = async () => {throw Error('unexpected single placement in regular batch corpus')};
    target.emitAndLogError = value => errors.push(value);
    target.recordRecentlyPlacedOrderFromOpts = (id, order, payload) => recorded.push([id, order.symbol, payload.qty]);
    target.xhr = {post:async (endpoint, payload) => {
      requests.push({endpoint,payload});
      assert(fixture.replies.length > 0, 'batch reply supplied');
      return {data:fixture.replies[Math.min(requests.length - 1, fixture.replies.length - 1)]};
    }};
    let result;
    try { result = {ids:await target.placeOrders(fixture.orders)}; }
    catch (error) {result = {error:error.message};}
    outputs.push({...result,requests,errors,sleeps,recorded});
  }
  assert.equal(outputs.length, 24);
  process.stdout.write(JSON.stringify(outputs));
})().catch(error => {console.error(error);process.exitCode=1});

}
