const {load,root}=require('../tests/configuration.cjs');
const path=require('node:path');
const {reverseWsSymbol}=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.utils.ts'));
const seeds=['','E','e','E_','E-','E_BTCUSDT','e-btc-usdc','EETHUSD','ETHUSDT','BTCUSDT','USDT','USDC','USD','E_E-TH_US-DT','E-BTC/USD',' E_BTCUSDT ','E-BTCUSDTT','ßusdt','ıusdc','ﬃusd','😀usd','e_é_usdt','e\n_btc_usdt'];
const rows=seeds.map(input=>({input,output:reverseWsSymbol(input)}));
// One corpus process. Exhaust all scalar uppercases, returning only changed
// scalars; Rust verifies both changed and unchanged cases on native strings.
const uppercase=[];
for(let code=0;code<=0x10ffff;code++) {
 if(code>=0xd800&&code<=0xdfff) continue;
 const input=String.fromCodePoint(code),upper=input.toUpperCase();
 if(input!==upper) uppercase.push([code,upper]);
}
const surrogate=reverseWsSymbol('\ud800usdt');
if(surrogate.charCodeAt(2)!==0xd800) throw Error('live utility surrogate witness');
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),rows,uppercase,surrogate}));
