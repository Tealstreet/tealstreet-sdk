import {sourceStore,} from './store_source.mjs';
import {loadSource} from './source_loader.mjs';
const {Store}=sourceStore();const rows=[];
const order=(id,price)=>({id,symbol:'BTC',status:'open',type:'limit',side:'buy',price,amount:3,filled:1,remaining:2,reduceOnly:false,lastUpdate:0});
for(const existingCount of [1,2])for(const incomingCount of [1,2]){
 const store=new Store();const existing=Array.from({length:existingCount},(_,i)=>order('duplicate',11+i*12));const incoming=Array.from({length:incomingCount},(_,i)=>order(i===0?'duplicate':'other',37+i*4));
 store.update({orders:existing});store.addOrUpdateOrders(incoming);
 rows.push({kind:'batch',existing,incoming,prices:store.orders.map(o=>o.price)});
}
const {calculateReturns}=loadSource(new URL('../../../packages/safe-cex/src/utils/roeRoi/calculateReturns.ts',import.meta.url));
const position={id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:10,liquidationPrice:0};
const bits=n=>Number.isNaN(n)?'7ff8000000000000':(()=>{const d=new DataView(new ArrayBuffer(8));d.setFloat64(0,n);return d.getBigUint64(0).toString(16).padStart(16,'0');})();
for(const [name,value] of [['plus-zero',0],['minus-zero',-0],['nan',NaN],['finite',-20]]){const output=calculateReturns(position,value);rows.push({kind:'returns',name,position,overrideBits:bits(value),roeBits:bits(output.roe),roiBits:bits(output.roi),captureArchitecture:process.platform+'-'+process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0])});}
console.log(JSON.stringify(rows));
