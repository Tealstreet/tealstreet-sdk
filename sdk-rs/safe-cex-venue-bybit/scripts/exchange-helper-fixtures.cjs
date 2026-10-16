// Execute the full maintained adapter and actual enum dependencies; observe native values.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..'), cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {}; cache.set(file, exports);
  let source = fs.readFileSync(file, 'utf8');
  vm.runInNewContext(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports,require(spec) {
    assert(spec.startsWith('.') || spec.startsWith('@tealstreet/'), `dependency ${spec}`);
    const target = spec.startsWith('.') ? path.resolve(path.dirname(file),spec) : path.join(root,'packages',spec.slice('@tealstreet/'.length));
    return load(fs.existsSync(target+'.ts') ? target+'.ts' : path.join(target,'index.ts'));
  }}, {filename:file}); return exports;
}
function observe(value) {
  if (value === undefined) return {undefined:true};
  if (typeof value === 'function') return {inherited:'function'};
  if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(value); return {number:Number.isNaN(value)?'nan':b.toString('hex')}; }
  if (Array.isArray(value)) return Array.from(value,observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
  return value;
}

const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
function subject(file,names){
 const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit',file),'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const declarations=new Map(ast.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).map(d=>[d.name.getText(ast),d.initializer]));
 const context=vm.createContext({OrderType:enums.OrderType,OrderStopTriggerType:enums.OrderStopTriggerType});
 for(const name of [...(names.includes('isBybitPositionTpslMode')?['BYBIT_POSITION_TPSL_MODES']:[]),...names]){
  assert(declarations.has(name),`source helper missing ${file}:${name}`);
  vm.runInContext(ts.transpileModule('globalThis.'+name+'='+declarations.get(name).getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 }
 return context;
}
const scalarNames=['stringOrUndefined','numberOrUndefined','stringFromUnknown','isBybitPositionTpslMode','isBybitOrderAlreadyGoneCancelMessage','isBybitOrderNotModifiedMessage','parseBybitFiniteNumber'];
const linear=subject('bybit.exchange.ts',[...scalarNames,'sumBybitCoinFields','deriveBybitAvailableBalanceFromCoins','firstFiniteNumber']);
const inverse=subject('bybitinverse.exchange.ts',['isBybitPositionTpslMode','isBybitOrderAlreadyGoneCancelMessage','isBybitOrderNotModifiedMessage','getBybitTriggerBy','getBybitTriggerDirectionPrice']);
const spot=subject('bybitspot.exchange.ts',['isBybitOrderAlreadyGoneCancelMessage']);
const values=[undefined,null,false,true,0,-0,3,-3,'',' ','Full','Partial','full','Partial ',' \uFEFF12.5 ','\u008512.5','0xff','12.5tail','Infinity','NaN','ORDER NOT EXISTS','Too late TO CANCEL','NOT MODIFIED','LEVERAGE not MODIFIED',[],[2],['Order not exists'],{}, {toString:null}];
assert.equal(values.length,29,'HELPER independent input floor');
const scalars=[];
for(const [family,api,names]of[['linear',linear,scalarNames],['inverse',inverse,['isBybitPositionTpslMode','isBybitOrderAlreadyGoneCancelMessage','isBybitOrderNotModifiedMessage']],['spot',spot,['isBybitOrderAlreadyGoneCancelMessage']]])for(const name of names)for(const value of values){
 let result;try{result={ok:observe(api[name](value))};}catch(e){result={error:e.name,message:e.message};}
 scalars.push({family,name,raw:value===undefined?null:Object.is(value,-0)?'-0':JSON.stringify(value),result});
}
assert.equal(scalars.length,319,'HELPER independent scalar floor');
const coinRows=[[],[{}],[{a:0,usdValue:0,walletBalance:30,locked:10}],[{a:'1.25tail',b:'2.5',usdValue:'17.5',totalPositionIM:'2.25',totalOrderIM:'3.5',locked:'1.125',bonus:'0.625'},{a:'-3.5',b:'0',walletBalance:'10.75',equity:99,locked:-2}], [{usdValue:'junk',walletBalance:'junk',equity:'12.5tail',totalPositionIM:2}], [{usdValue:2,walletBalance:{toString:null},locked:3}], [{locked:{toString:null}}], [{a:1e308,b:1e308,usdValue:1e308,totalPositionIM:1e308,totalOrderIM:1e308,locked:-1e308,bonus:-1e308}]];
const coins=[];
for(const rows of coinRows)for(const name of ['sumBybitCoinFields','deriveBybitAvailableBalanceFromCoins']){
 let result;try{result={ok:observe(linear[name](rows,['a','b']))};}catch(e){result={error:e.name,message:e.message};}coins.push({name,rows,result});
}
assert.equal(coins.length,16,'HELPER independent coin floor');
const first=[['undefined'],['NaN','Infinity','-Infinity'],['undefined','-0','2.5'],['NaN','0','2.5'],['Infinity','-3.75','4'],['1','2']].map(tokens=>({tokens,result:observe(linear.firstFiniteNumber(...tokens.map(t=>t==='undefined'?undefined:Number(t))))}));
assert.equal(first.length,6,'HELPER first-number floor');
const ticker={id:'ticker',symbol:'BTC',bid:1,ask:2,last:17.25,mark:23.5,index:31.75,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const triggers=[];
for(const type of Object.values(enums.OrderType))for(const trigger of [undefined,...Object.values(enums.OrderStopTriggerType)]){
 const opts={symbol:'BTC',type,side:'buy',amount:3,triggerType:trigger,slTriggerType:trigger,tpTriggerType:trigger};
 triggers.push({opts,ticker,by:observe(inverse.getBybitTriggerBy(opts)),price:observe(inverse.getBybitTriggerDirectionPrice(opts,ticker))});
}
assert.equal(triggers.length,24,'HELPER inverse trigger floor');
const boundary={rows:[null],fields:[],sourceSum:observe(linear.sumBybitCoinFields([null],[]))};
process.stdout.write(JSON.stringify({scalars,coins,first,triggers,boundary}));
