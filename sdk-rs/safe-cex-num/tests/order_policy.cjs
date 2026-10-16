const {root,load}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const rounding=load(root+'/packages/script-api-core/src/order/rounding.ts');
const be=load(root+'/packages/script-api-core/src/order/breakevenPrice.ts');
const notional=load(root+'/packages/script-api-core/src/order/notionalSizing.ts');
const down=load(root+'/packages/script-api-core/src/order/shouldRoundSizeDown.ts');
const bits=n=>{const bytes=Buffer.alloc(8);bytes.writeDoubleBE(Number.isNaN(n)?NaN:n);return bytes.toString('hex');};
const rows=[];
const numbers=[0,-0,0.3,0.35,-0.5,1.005,2.9,0.29999999,1e-11,5e-324,1e21,1.0000000000000002,NaN,Infinity,-Infinity];
const steps=[0,-0,0.1,0.01,1e-8,-0.1,NaN,Infinity,-Infinity];
for(const value of numbers)for(const step of steps)for(const op of ['roundToStep','roundMarketOrderSize','roundCloseSize'])for(const reduceOnly of [false,true]){
 const expected=op==='roundMarketOrderSize'?rounding[op](value,step,{reduceOnly}):rounding[op](value,step);
 rows.push({op,value:bits(value),step:bits(step),reduceOnly,expected:bits(expected)});
}
const entries=[100,-0,5e-324,1.0000000000000002,1e21,NaN,Infinity,-Infinity];
const sizes=[0,-0,10,-10,5e-324,NaN,Infinity,-Infinity];
const nets=[undefined,0,-0,50,-10,NaN,Infinity,-Infinity];
for(const entryPrice of entries)for(const size of sizes)for(const side of ['long','short'])for(const bankedPnl of nets)for(const fees of nets){
 rows.push({op:'breakevenPrice',entryPrice:bits(entryPrice),size:bits(size),side,bankedPnl:bankedPnl===undefined?null:bits(bankedPnl),fees:fees===undefined?null:bits(fees),expected:bits(be.breakevenPrice({entryPrice,size,side,bankedPnl,fees}))});
}
const notionals=[0,-0,10,-10,NaN,Infinity,-Infinity];
const contracts=[undefined,0,-0,0.0001,1000,-1,NaN,Infinity,-Infinity];
const prices=[0,-0,100,NaN,Infinity,-Infinity];
for(const n of notionals)for(const cs of contracts)for(const isInverse of [false,true])for(const price of prices){
 rows.push({op:'notionalToNativeSize',notional:bits(n),contractSize:cs===undefined?null:bits(cs),isInverse,price:bits(price),expected:bits(notional.notionalToNativeSize(n,{contractSize:cs,isInverse},price))});
}
for(const isSpot of [undefined,false,true])for(const exchangeName of [undefined,'','hyperliquid','hyperliquidspot','bybit','coinbasespot','unknown']){
 rows.push({op:'shouldRoundSizeDown',isSpot:isSpot??null,exchangeName:exchangeName??null,expected:down.shouldRoundSizeDown({isSpot},exchangeName)});
}
process.stdout.write(JSON.stringify(rows));
