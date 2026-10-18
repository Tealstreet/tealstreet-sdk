const assert = require('node:assert/strict');
const path = require('node:path');
const {load, root} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const source = load(path.join(root, 'packages/safe-cex/src/utils/signatureRejection.ts'));
const output = [];
for (const rejected of [true, false]) {
  const config = {url: '/source', timeout: 27};
  const data = {code: {toString: 0}};
  let requests = 0;
  try {
    if (rejected) source.signatureRejectedResponse(source.isBinanceStyleSignatureRejection)({config, response: {data}});
    else source.resendOnSignatureRejection({request() { requests++; throw Error('must not resend'); }}, {config, data}, source.isBinanceStyleSignatureRejection);
    assert.fail('source predicate must throw');
  } catch (error) {
    assert.equal(error.name, 'TypeError');
    output.push({name: error.name, message: error.message, stamped: source.hasRetriedSignature(config), requests});
  }
}
console.log(JSON.stringify(output));
