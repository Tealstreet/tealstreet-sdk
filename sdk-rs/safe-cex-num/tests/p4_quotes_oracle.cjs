const path=require('node:path');const {root,load}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const ops=['baseToNativeContracts','getTickerPredictedBestAsk','getTickerPredictedBestBid'];
const f=Object.fromEntries(ops.map(op=>[op,load(path.join(root,'packages/safe-cex/src/utils',op+'.ts'))[op]]));
const values=[-Infinity,-2,-0,0,0.001,2,Infinity,NaN],enc=v=>Number.isNaN(v)?'NaN':Object.is(v,-0)?'-0':String(v),rows=[];
for(const size of values)for(const base of values)rows.push({op:ops[0],size:enc(size),base:enc(base),expected:enc(f[ops[0]]({contractSize:size},base))});
for(const last of values)for(const quote of values)for(const price of [...values,null])for(const op of ops.slice(1))rows.push({op,last:enc(last),quote:enc(quote),price:price===null?null:enc(price),expected:enc(f[op]({last,bid:quote,ask:quote},price===null?null:{precision:{price}}))});
process.stdout.write(JSON.stringify(rows));
