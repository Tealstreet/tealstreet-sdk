const assert=require('node:assert/strict'),path=require('node:path');
const {root,load}=require('../scripts/accounts-source.cjs');
const m=load(path.join(root,'packages/safe-cex/src/exchangeMetadata.ts'));
assert.equal(Object.keys(m.exchangeMetadata).length,49); assert.equal(m.exchangeMetadata.bybit.prettyName,'Bybit');
assert.equal(m.isExchangeSupportedOnMobile('bybit'),true);assert.equal(m.isExchangeSupportedOnMobile('unknown'),false);
const unified=m.getExchangeMetadata('bybit',{isUnified:true});assert.notDeepEqual(unified,m.exchangeMetadata.bybit);
console.log(JSON.stringify({metadata:m.exchangeMetadata,unified,domains:m.getUniqueLatencyDomains(),groups:[...m.latencyDomainToExchanges]}));
