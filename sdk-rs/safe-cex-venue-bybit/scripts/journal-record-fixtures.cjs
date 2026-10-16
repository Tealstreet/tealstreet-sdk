// Execute the full maintained adapter and actual enum dependencies; observe native values.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..'), cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {}; cache.set(file, exports);
  let source = fs.readFileSync(file, 'utf8');
  if (file.endsWith('bybit.position-adapter.ts')) source += '\nexport { num, foldRun };';
  vm.runInNewContext(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports,require(spec) {
    assert(spec.startsWith('.') || spec.startsWith('@tealstreet/'), `dependency ${spec}`);
    const target = spec.startsWith('.') ? path.resolve(path.dirname(file),spec) : path.join(root,'packages',spec.slice('@tealstreet/'.length));
    return load(fs.existsSync(target+'.ts') ? target+'.ts' : path.join(target,'index.ts'));
  }}, {filename:file}); return exports;
}
const api=load(path.join(root,'packages/safe-cex/src/journal/bybit.position-adapter.ts'));
function observe(value) {
  if (value === undefined) return {undefined:true};
  if (typeof value === 'function') return {inherited:'function'};
  if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(value); return {number:Number.isNaN(value)?'nan':b.toString('hex')}; }
  if (Array.isArray(value)) return Array.from(value,observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
  return value;
}
const base={symbol:'DOGEUSDT',orderId:'one',side:'Sell',avgEntryPrice:'2.25',closedSize:'3.5',openFee:'0.125',closeFee:'0.25',closedPnl:'7.5',avgExitPrice:'4.75',cumExitValue:'16.625',updatedTime:'1700000000123',createdTime:'1690000000987',leverage:'8',execType:'Trade'};
const values=[undefined,null,false,true,0,-0,-1,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const cases=[];
function add(rows, mode='joined', op='aggregate') {
  const trace=[];
  const deps={account:'acct',resolveSettleCurrency:s=>{trace.push(['currency',s]);return s==='BTCUSDC'?'USDC':'USDT';}};
  if(mode!=='plain' && mode!=='context-only') deps.resolveLeg=id=>{trace.push(['leg',id]);return id==='two'?'short':undefined;};
  if(mode!=='plain' && mode!=='leg-only') deps.resolveContext=arg=>{trace.push(['context',arg.symbol,arg.positionSide,arg.closedAt]);return mode==='missing'?undefined:{openedAt:1699990000321,funding:-0.625,fundingAttributed:mode!=='unattributed'};};
  let result;
  try {const value = op==='fold' ? api.foldRun(rows,deps) : api.aggregateBybitClosedPnl(rows,deps); result={ok:observe(op==='fold' ? value===null ? [] : [value] : value)};} catch(e) {result={error:e.name,message:e.message};}
  cases.push({rows,mode,op,trace:observe(trace),result});
}
for(const key of Object.keys(base)) for(const value of values) {
  const row={...base}; if(value===undefined)delete row[key];else row[key]=value;
  add([row]);add([base,{...row,orderId:'same-leg'}]);
}
for(const mode of ['joined','plain','missing','unattributed','context-only','leg-only']) add([base,{...base,symbol:'BTCUSDC',orderId:'btc',updatedTime:'1700000000111'},{...base,orderId:'same-leg',avgEntryPrice:'2.250',updatedTime:'1700009999999'},{...base,orderId:'two'}],mode);
for(const reason of ['Trade','AdlTrade','BustTrade','Settle','unknown',...Object.getOwnPropertyNames(Object.prototype)]) add([{...base,execType:reason}]);
add([]);add([null]);add([null],'plain');add([{...base,symbol:''}]);add([{...base,closedSize:0}]);
add([{...base,orderId:{toString:null}}],'plain'); // optional leg call must skip its arguments.
add([{...base,execType:'constructor'},{...base,symbol:'LATER',openFee:{toString:null}}]); // later source error outranks output refusal.
add([{...base,orderId:{toString:null},closedSize:0}],'plain'); // Discriminates eager optional arguments: source returns [] before key coercion.
for(const rows of [[],[null],[{...base,symbol:''}],[base]]) add(rows,'plain','fold');
const numbers=values.map(v=>{try{return {raw:v===undefined?null:JSON.stringify(v),result:{ok:observe(api.num(v))}};}catch(e){return {raw:JSON.stringify(v),result:{error:e.name,message:e.message}};}});
assert.equal(cases.length,595,'JOURNAL independent source case floor');
assert.equal(numbers.length,20,'JOURNAL independent numeric floor');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.versions.node},cases,numbers}));
