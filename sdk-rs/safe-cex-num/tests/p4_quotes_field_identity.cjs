// Field-identity supplement: original p4_quotes_oracle.cjs stays byte-identical.
const path = require('node:path');
const {root,load} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const {bits} = require('./support/number-bits.cjs');
const names = ['getTickerPredictedBestAsk','getTickerPredictedBestBid'];
const functions = names.map(name => load(path.join(root,'packages/safe-cex/src/utils',name+'.ts'))[name]);
const rows = [];
for (const last of [0,13,NaN]) for (const bid of [0,7,11]) for (const ask of [0,17,19]) for (const price of [null,0.5,2]) {
  const ticker = {last,bid,ask};
  const market = price === null ? null : {precision:{price}};
  for (let i=0;i<names.length;i++) rows.push({op:names[i], last:bits(last),bid:bits(bid),ask:bits(ask),price,expected:bits(functions[i](ticker,market))});
}
process.stdout.write(JSON.stringify(rows));
