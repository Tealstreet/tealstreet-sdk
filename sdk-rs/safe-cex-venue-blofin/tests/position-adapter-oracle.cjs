// Execute the full maintained adapter and actual enum dependencies; observe native values.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..'), cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {}; cache.set(file, exports);
  let source = fs.readFileSync(file, 'utf8');
  if (file.endsWith('.position-adapter.ts')) source += '\nexport { num };';
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

const venue=process.argv[2];assert(['blofin','bitunix'].includes(venue));
const api=load(path.join(root,'packages/safe-cex/src/journal/'+venue+'.position-adapter.ts'));
const base=venue==='blofin'?{instId:'DOGE-USDT',positionSide:'long',closePositions:'3.5',maxPositions:'7.25',fee:'-0.125',realizedPnl:'2.75',updateTime:'1700000000456',createTime:'1699990000123',liquidationPositions:'0',historyId:'round',positionId:'shared',leverage:'8.25',openAveragePrice:'11.25',closeAveragePrice:'13.75'}:{symbol:'DOGEUSDT',side:'BUY',qty:'3.5',maxQty:'7.25',fee:'0.125',funding:'-0.625',realizedPNL:'2.75',mtime:'1700000000456',ctime:'1699990000123',liqQty:'0',positionId:'round',leverage:'8.25',entryPrice:'11.25',closePrice:'13.75',positionMode:'HEDGE'};
const values=[undefined,null,0,'-2.5','12.5tail',{toString:null}];const cases=[];
function add(row,flag=false,mode='ordinary'){
 const trace=[];
 const deps={account:'acct',resolveSettleCurrency:s=>{trace.push(['currency',s]);return mode==='empty-currency'?'':'USDC';}};
 if(venue==='blofin') {deps.resolveSymbol=s=>{trace.push(['symbol',s]);return mode==='unknown'?undefined:'mapped:'+s;};if(flag)deps.contractSize=s=>{trace.push(['scale',s]);return mode==='zero'?0:mode==='nan'?NaN:mode==='negative'?-1:2.5;};}
 else deps.realizedIsNetOfFunding=flag;
 let result;try{result={ok:observe(venue==='blofin'?api.mapBlofinPositionHistory(row,deps):api.mapBitunixPositionHistory(row,deps))};}catch(e){result={error:e.name,message:e.message};}
 cases.push({row,flag,mode,trace,result});
}
for(const key of Object.keys(base))for(const value of values)for(const flag of [false,true]){const row={...base};if(value===undefined)delete row[key];else row[key]=value;add(row,flag);}
for(const flag of [false,true]){add(base,flag);add(null,flag);add({...base,[venue==='blofin'?'positionSide':'side']:'SHORT'},flag);add({...base,[venue==='blofin'?'liquidationPositions':'liqQty']:'-1'},flag);}
if(venue==='blofin')for(const mode of ['unknown','empty-currency','zero','nan','negative'])add(base,true,mode);
if(venue==='bitunix')add({...base,side:'sell'},true); // Exercise successful closing direction independently of invalid-side refusal.
const numbers=values.map(v=>{try{return {raw:v===undefined?null:JSON.stringify(v),result:{ok:observe(api.num(v))}};}catch(e){return {raw:JSON.stringify(v),result:{error:e.name,message:e.message}};}});
assert.equal(cases.length,venue==='blofin'?181:189,'ADAPTER independent source floor');assert.equal(numbers.length,6,'ADAPTER numeric floor');
process.stdout.write(JSON.stringify({cases,numbers}));
