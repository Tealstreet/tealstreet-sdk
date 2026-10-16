const {write}=require('./load-starbase.cjs');
const {captureHeader,numberBits}=require('./special-numbers.cjs');
const cases=[];
for(const savedSymbol of ['', 'BTC-PERP-INTX','ETH-PERP-INTX','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL','SOL-PERP-INTX','BTC-USD',' btc-perp-intx','BTC-PERP-INTX\n','\uFEFFBTC-PERP-INTX','constructor'])for(const type of ['limit','market'])for(const price of [undefined,-0,0,-1,1,0.5,Number.MIN_VALUE,Number.MAX_VALUE,NaN,Infinity,-Infinity]){
 const intent={price,savedSymbol,type};let result;try{result={ok:write.buildCoinbaseStarbaseClosePosition(intent)};}catch(error){result={error:{name:error.name,message:error.message}};}
 cases.push({...captureHeader(),intent,...price===undefined?{}:{priceBits:numberBits(price)},result});
}
process.stdout.write(JSON.stringify(cases));
