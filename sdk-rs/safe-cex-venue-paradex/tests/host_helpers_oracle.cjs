const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH), root = path.resolve(__dirname, '../../..');
function parse(file) {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
  assert.equal(source.parseDiagnostics.length, 0, 'helpers_source_parse'); return source;
}
function execute(code, globals = {}) {
  const module = { exports: {} };
  const out = ts.transpileModule(code, { reportDiagnostics: true, compilerOptions: { module: 1, target: 9 } });
  assert.equal(out.diagnostics.length, 0, 'helpers_transpile');
  vm.runInNewContext(out.outputText, { module, exports: module.exports, ...globals }); return module.exports;
}
function members(source, className, names) {
  const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === className);
  return names.map(name => {
    const matches = cls.members.filter(n => n.name?.getText(source) === name);
    assert.equal(matches.length, 1, 'helpers_member_identity'); return matches[0].getText(source);
  }).join('\n');
}
const exchange = parse('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const shared = parse('packages/safe-cex/src/types.ts');
const enums = execute(shared.statements.filter(n => ts.isEnumDeclaration(n) && ['OrderType', 'OrderTimeInForce'].includes(n.name.text)).map(n => n.getText(shared)).join('\n'));
const { Exchange } = execute(`export class Exchange { ${members(exchange, 'ParadexExchange', ['isNativeBatchOrder', 'listenOrderBook', 'listenOHLCV'])} }`, enums);
const sdk = parse('packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const { Signer } = execute(`export class Signer { ${members(sdk, 'ParadexRequestSigner', ['getAccountAddress'])} }`);
const { Manager } = execute(`export class Manager { ${members(sdk, 'ParadexJWTManager', ['clearToken'])} }`);
const account = sdk.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === 'ParadexSDKAccount');
assert.equal(account.members.length, 3, 'sdk_shape_member_count');
assert.ok(!account.heritageClauses?.length, 'sdk_shape_heritage');
assert.deepEqual(account.members.map(n => {
  assert.ok(ts.isPropertySignature(n) && ts.isIdentifier(n.name) && !n.questionToken && !n.modifiers?.length, 'sdk_shape_member');
  assert.equal(n.type.kind, ts.SyntaxKind.AnyKeyword, 'sdk_shape_any'); return n.name.text;
}), ['account', 'config', 'provider'], 'sdk_shape_fields');
const identityCode = `${account.getText(sdk)}\nexport function identity(value: ParadexSDKAccount): ParadexSDKAccount { return value; }`;
const virtual = path.join(root, '.aimux/paradex/sdk-account-witness.ts');
const host = ts.createCompilerHost({strict:true,target:9});
const getSourceFile = host.getSourceFile.bind(host);
host.getSourceFile = (file, lang, onError, create) => file === virtual ? ts.createSourceFile(file, identityCode, lang, true) : getSourceFile(file, lang, onError, create);
const program = ts.createProgram([virtual], {strict:true,target:9,noEmit:true,types:[],skipLibCheck:true}, host);
assert.equal(ts.getPreEmitDiagnostics(program).length, 0, 'sdk_identity_typecheck');
const { identity } = execute(identityCode);
const sdkRecord = {account:{tag:'account'},config:{tag:'config'},provider:{tag:'provider'}};
const sdkResult = identity(sdkRecord);
const sdkIdentity = ['account','config','provider'].map(key => ({key,tag:sdkResult[key].tag,same:sdkResult[key]===sdkRecord[key]}));
const h = new Exchange(), batch = [];
const base = {type:'limit'};
function add(patch = {}, remove = []) {const value={...base,...patch}; for(const key of remove)delete value[key]; batch.push({input:value,output:h.isNativeBatchOrder(value)});}
for(const type of ['market','limit','conditional','stop_market','take_profit_market','trailing_stop_market'])add({type});
for(const timeInForce of [undefined,'GoodTillCancel','ImmediateOrCancel','PostOnly','FillOrKill'])add({timeInForce});
for(const close of [undefined,false,true])add({close});
for(const field of ['triggerPrice','stopLoss','takeProfit'])for(const value of [undefined,0,-0,17.25,NaN,Infinity])add({[field]:value});
for(const field of ['tpTriggerType','slTriggerType'])for(const value of [undefined,'mark_price','last_price','index_price'])add({[field]:value});
add({triggerPrice:0,stopLoss:0,takeProfit:0,tpTriggerType:'mark_price',slTriggerType:'last_price',close:true,timeInForce:'FillOrKill'});
add({symbol:String.fromCharCode(0xd800)});
function number(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');}
const batchInputs = batch.map(c=>({fields:Object.entries(c.input).map(([key,value])=>({key,kind:typeof value,value:typeof value==='number'?number(value):typeof value==='string'?[...value].map(ch=>ch.charCodeAt(0)):value})),output:c.output}));
const delegations=[];
for(const method of ['listenOrderBook','listenOHLCV'])for(const mode of ['return','throw','callback']){
  const input = method==='listenOrderBook' ? String.fromCharCode(0xd800) : {symbol:String.fromCharCode(0xd800),interval:'5m',extra:{marker:'keep'}};
  const returned={tag:'subscription'},error={tag:'delegate-error'},event={tag:'event'},trace=[];
  const callback=value=>{trace.push(['callback',value===event]);return 'callback-result';};
  h.publicWebsocket={[method](arg,cb){trace.push(['delegate',arg===input,cb===callback]);if(mode==='throw')throw error;if(mode==='callback')trace.push(['callback-return',cb(event)]);return returned;}};
  let result;
  try{const value=h[method](input,callback);result={ok:true,same:value===returned};}catch(e){result={ok:false,same:e===error};}
  delegations.push({method,mode,result,trace});
}
const addresses=[];
for(const mode of ['read','changed','throw','account-throw']){
  const signer=new Signer(),original=String.fromCharCode(0xd800),changed='changed',error={tag:'address-error'};let reads=0,accountReads=0;
  const account={get address(){reads++;if(mode==='throw')throw error;return reads===1?original:changed;}};
  signer.sdkAccount={get account(){accountReads++;if(mode==='account-throw')throw error;return mode==='changed'?{get address(){reads++;return changed;}}:account;},config:{tag:'config'},provider:{tag:'provider'}};
  let result;try{const value=signer.getAccountAddress();result={ok:true,units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};}catch(e){result={ok:false,same:e===error};}
  addresses.push({mode,reads,accountReads,result});
}
const caches=[];
for(const expiry of [0,-0,123456.75,Infinity,NaN])for(const populated of [false,true]){
  const manager=new Manager(),refresh={tag:'refresh'},token=String.fromCharCode(0xd800);
  manager.token=populated?token:null;manager.tokenExpiry=expiry;manager.refreshPromise=populated?refresh:null;
  const result=manager.clearToken();caches.push({expiry:number(expiry),populated,tokenNull:manager.token===null,expiryBits:number(manager.tokenExpiry),refreshNull:manager.refreshPromise===null,undefined:result===undefined,originalRefresh:refresh.tag});
}
process.stdout.write(JSON.stringify({header:{arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0])},sdkIdentity,batch:batchInputs,delegations,addresses,caches}));
