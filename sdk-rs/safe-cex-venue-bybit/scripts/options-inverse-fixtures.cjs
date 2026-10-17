const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
// Both modules carry type-only imports, so they execute with no runtime dependencies.
function load(file){const text=fs.readFileSync(path.join(root,file),'utf8');const out=ts.transpileModule(text,{compilerOptions:{module:1,target:9}}).outputText;const exports={};vm.runInNewContext(out,{exports,require(){throw Error('no runtime imports expected')}});return exports;}
const options=load('packages/safe-cex/src/exchanges/bybit/bybit.options.ts');
const inverse=load('packages/safe-cex/src/exchanges/bybit/bybit.inverse.ts');
const symbols=['BTCUSD','ETHUSDZ26','BTCUSDT','BTCUSDC','USD','USDZ26','BTCUSDZ2','BTCUSDA26','btcusd','BTC-USD','1000PEPEUSD','BTCUSDH27',
 'BTC-25SEP26-130000-C-USDT','NVDA-30OCT26-247.5-P-USDT','SOL-6NOV26-112d5-P','BTC-25SEP26-130000-C','BTC-125SEP26-1-C','BTC-25Sep26-1-C',
 'BTC-25SEP26-1-X','BTC-25SEP26-1.-C','BTC-25SEP26-d5-C','BTC-25SEP26-1-C-usdt','-25SEP26-1-C','BTC-5SEP26-1d5-P-USDC','BTC-25SEP2026-1-C','BTC-25SEP26-1-C-USDT-X'];
const rows=[
 {status:'Trading',symbol:'BTC-25SEP26-130000-C-USDT',optionsType:'Call',baseCoin:'BTC',quoteCoin:'USDT',settleCoin:'USDT',deliveryTime:'1790323200000',lotSizeFilter:{qtyStep:'0.01',minOrderQty:'0.01',maxOrderQty:'500'},priceFilter:{tickSize:'5',minPrice:'5'}},
 {status:'Trading',symbol:'SOL-6NOV26-112d5-P',optionsType:'Put',baseCoin:'SOL',quoteCoin:'USDC',settleCoin:'USDC',deliveryTime:'1793952000000',lotSizeFilter:{qtyStep:'0.1'},priceFilter:{}},
 {status:'Trading',symbol:'BTC-27DEC26-90000-P',optionsType:'Put',baseCoin:'BTC',quoteCoin:'USDC',settleCoin:'USDC',deliveryTime:'1798358400000',lotSizeFilter:null,priceFilter:{tickSize:'0.5'}},
 {status:'Trading',symbol:'BTC-25SEP26-1-C',optionsType:'Call',baseCoin:'BTC',quoteCoin:'USDC',settleCoin:'USDC',deliveryTime:'1790323200000',lotSizeFilter:{},priceFilter:{}},
 {status:'Delivering',symbol:'BTC-25SEP26-1-C',optionsType:'Call'},
 {status:'Trading',symbol:'BTC-25SEP26-0-C',optionsType:'Call'},
 {status:'Trading',symbol:'BTC-25SEP26-1-C',optionsType:'Straddle'},
 {status:'Trading',symbol:'BTC',optionsType:'Put',baseCoin:'BTC'},
];
const positions=[
 {symbol:'BTCUSD',unrealizedPnl:0.01,notional:1000,contracts:0.02},{symbol:'BTCUSD',unrealizedPnl:0.01,notional:1000,contracts:0},
 {symbol:'BTCUSDT',unrealizedPnl:5,notional:1000,contracts:0.02},{symbol:'ETHUSDZ26',unrealizedPnl:NaN,notional:10,contracts:2},
 {symbol:'BTCUSD',unrealizedPnl:-0.5,notional:600,contracts:0.01}];
const out={
 symbols:symbols.map(s=>({s,inverse:inverse.isBybitInverseSymbol(s),option:options.isBybitOptionSymbol(s)})),
 rows:rows.map(r=>({raw:r,market:options.mapBybitOptionInstrument(r)??null})),
 positions:positions.map(p=>({p:{...p,unrealizedPnl:Number.isNaN(p.unrealizedPnl)?'NaN':p.unrealizedPnl},upnl:inverse.positionUpnlInQuote(p)})),
 units:options.BYBIT_GREEK_UNITS,
};
assert.equal(out.symbols.length,26,'symbol floor');
assert.equal(out.rows.length,8,'row floor');
process.stdout.write(JSON.stringify(out));
