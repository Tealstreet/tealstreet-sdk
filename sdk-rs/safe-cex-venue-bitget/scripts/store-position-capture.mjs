// Actual shared TS Store operations; no live venue, timer or network.
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const {Store}=sourceStore();
const encode=n=>({$number:Object.is(n,-0)?'-0':String(n)});
const position=price=>({id:'p',symbol:'BTCUSDT',side:'long',contracts:2,entryPrice:10,notional:20,leverage:1,unrealizedPnl:0,liquidationPrice:price});
const output=[];
for(const price of [0,-0,17,NaN,Infinity,-Infinity]){
 const fresh=new Store();fresh.addOrUpdatePosition(position(price));
 const inserted=fresh.positions[0].liquidationPrice;
 let unrelatedError=null;try{fresh.updatePosition({id:'p'},{contracts:3});}catch(e){unrelatedError={name:e.name,message:e.message};}
 const patched=new Store();patched.addOrUpdatePosition(position(17));
 let patchError=null;try{patched.updatePosition({id:'p'},{liquidationPrice:price});}catch(e){patchError={name:e.name,message:e.message};}
 output.push({input:encode(price),inserted:encode(inserted),unrelatedError,afterUnrelated:encode(fresh.positions[0].liquidationPrice),patchError,patched:encode(patched.positions[0].liquidationPrice)});
}
process.stdout.write(JSON.stringify(output));
