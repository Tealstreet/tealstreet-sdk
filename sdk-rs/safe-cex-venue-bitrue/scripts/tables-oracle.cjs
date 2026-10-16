const {load,root}=require('../tests/configuration.cjs');
const path=require('node:path');
const actual=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
const rows=[];
for(const name of ['INTERVAL','ORDER_TYPE','ORDER_STATUS','ORDER_SIDE']) {
 const table=actual[name];
 for(const key of [...Object.keys(table),...Object.getOwnPropertyNames(Object.prototype),'missing','', '1M']) {
  const value=table[key];
  rows.push({name,key,result:typeof value==='undefined'?{kind:'missing'}:Object.hasOwn(table,key)?{kind:'mapped',value}:{kind:'inherited',value:String(value),type:typeof value}});
 }
}
process.stderr.write(JSON.stringify({capture_header:require('./capture-header.cjs')()})+'\n');
process.stdout.write(JSON.stringify(rows));
