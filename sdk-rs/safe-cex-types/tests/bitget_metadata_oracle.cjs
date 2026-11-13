const path = require('node:path');
const { root, load } = require('../scripts/accounts-source.cjs');
const { getExchangeMetadata } = load(path.join(root, 'packages/safe-cex/src/exchangeMetadata.ts'));

const cases = [];
for (const exchange of ['bitget', 'bitgetusdc', 'bitgetinverse']) {
  for (const bitgetAccountMode of [undefined, 'classic', 'uta', 'UTA']) {
    for (const tpslOrderPath of [undefined, 'entry', 'position', 'unsupported']) {
      for (const isUnified of [false, true]) {
        const options = { bitgetAccountMode, tpslOrderPath, isUnified };
        cases.push({ exchange, options, metadata: getExchangeMetadata(exchange, options) });
      }
    }
  }
}
console.log(JSON.stringify(cases));
