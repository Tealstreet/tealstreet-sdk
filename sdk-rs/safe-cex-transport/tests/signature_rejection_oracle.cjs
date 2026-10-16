// Executes the actual utils/signatureRejection.ts; one process returns the whole corpus.
const assert = require('node:assert/strict');
const path = require('node:path');
const { load, root } = require('../../safe-cex-types/scripts/accounts-source.cjs');
const source = load(path.join(root, 'packages/safe-cex/src/utils/signatureRejection.ts'));
const inputs = JSON.parse(process.argv[2]);
const outcome = (run) => {
  try {
    return { value: run() };
  } catch (error) {
    return { thrown: error.name };
  }
};
const binance = inputs.binance.map((data) => outcome(() => source.isBinanceStyleSignatureRejection(data)));
// Each case is one call: a condition sequence over the same config, then the resends it caused.
const sequences = inputs.sequences.map(({ errors, responses }) => {
  const config = { url: '/order' };
  let calls = 0;
  const isRejection = (data) => {
    calls += 1;
    return source.isBinanceStyleSignatureRejection(data);
  };
  const condition = source.signatureRejectedResponse(isRejection);
  const retried = errors.map((data) =>
    outcome(() => condition({ config, response: 'data' in data ? { data: data.data } : undefined })),
  );
  const sent = [];
  const xhr = { request: (next) => (sent.push(next), Promise.resolve('resent')) };
  // A resend's response carries the config that was actually sent.
  let current = config;
  const resent = responses.map((data) =>
    outcome(() => {
      const resend = source.resendOnSignatureRejection(xhr, { config: current, data }, isRejection);
      if (resend !== undefined) current = sent.at(-1);
      return resend !== undefined;
    }),
  );
  return { retried, resent, sent: sent.map((next) => next[source.SIGNATURE_RETRY_CONFIG_KEY] === true), calls };
});
assert.equal(source.SIGNATURE_RETRY_CONFIG_KEY, 'safeCexSignatureRetry');
assert.equal(source.hasRetriedSignature(undefined), false);
console.log(JSON.stringify({ key: source.SIGNATURE_RETRY_CONFIG_KEY, binance, sequences }));
