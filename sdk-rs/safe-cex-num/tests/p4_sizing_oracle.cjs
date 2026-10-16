const fs=require('node:fs'),path=require('node:path'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..'),cache=new Map();
function load(filename){if(cache.has(filename))return cache.get(filename).exports;const module={exports:{}};cache.set(filename,module);const code=ts.transpileModule(fs.readFileSync(filename,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;const localRequire=name=>name.startsWith('@tealstreet/math/src/')?load(path.join(root,'packages/math/src',name.slice('@tealstreet/math/src/'.length)+'.ts')):name.startsWith('.')?load(path.resolve(path.dirname(filename),name)+'.ts'):require(name);new Function('require','module','exports',code)(localRequire,module,module.exports);return module.exports;}
const sizing=load(path.join(root,'packages/safe-cex/src/utils/sizing.ts'));
const leverage=load(path.join(root,'packages/safe-cex/src/utils/leverage.ts'));
const slice=load(path.join(root,'packages/safe-cex/src/utils/getBalanceSlice.ts'));
const total=load(path.join(root,'packages/safe-cex/src/utils/getTotalBalanceFromBalance.ts'));
const ops=Object.keys(sizing),rows=[],encode=x=>typeof x==='number'?(Number.isNaN(x)?'NaN':Object.is(x,-0)?'-0':String(x)):x;
console.error=()=>{};
for(const inverse of [false,true])for(const quanto of [false,true])for(const cs of [0,1,0.001])for(const size of [-1,0,-0,0.3,1e-11,123.456,NaN,Infinity,-Infinity])for(const btc of [null,0,30000]) {
 const market={id:'m',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,isInverse:inverse,isSpot:false,contractSize:cs,precision:{amount:0.01,price:0.1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},info:{isQuanto:quanto}};
 const price=31.12345,entry=30.5,stop=29.9;
 for(const op of ops){let args;if(op==='getSizeCurrency')args=[market];else if(op==='getMarginCurrency')args=[market,'fallback'];else if(op==='getContractType')args=[market];else if(op==='calculatePnlInMarginCurrency')args=[size,entry,price,size>0,market,btc??undefined];else if(op==='convertMarginRiskToNativeSize')args=[size,entry,stop,market,btc??undefined];else args=[size,market,price,btc??undefined];rows.push({op,market,size:encode(size),price,entry,stop,btc,expected:encode(sizing[op](...args))});}
}
for(const notionals of [[],[-1,2,-3],[1e-11,4.44]])for(const balance of [-1,0,100])for(const op of Object.keys(leverage)){const args=op==='calculatePositionAccountLeverage'?[notionals[0]??0,balance]:op==='calculateTotalNotional'?[notionals.map(notional=>({notional}))]:[notionals.map(notional=>({notional})),balance];rows.push({op,notionals,balance,expected:encode(leverage[op](...args))});}
for(const market of [null,{base:'BTC',quote:'USDT',isInverse:false},{base:'BTC',quote:'USDT',isInverse:true},{base:'BTC',quote:'USDT',isSpot:true}])for(const balance of [null,{used:1,free:2,total:3,upnl:4},{used:1,free:2,total:3,upnl:4,currencies:{'':{used:9,free:8,total:99,upnl:0},BTC:{used:5,free:6,total:7,upnl:8}}}]) {for(const op of ['getBalanceSlice','getTotalBalanceFromBalance'])rows.push({op,market,balance,expected:encode(op==='getBalanceSlice'?slice[op](balance,market):total[op](balance,market))});}
process.stdout.write(JSON.stringify(rows));
