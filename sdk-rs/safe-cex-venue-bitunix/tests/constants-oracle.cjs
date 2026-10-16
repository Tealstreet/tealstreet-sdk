'use strict';
const { load, root, path } = require('./oracle-loader.cjs');
const constants = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const inherited = Object.getOwnPropertyNames(Object.prototype);
function observe(value) {
  // Capture typeof, inherited identity and Number bits before JSON can erase them.
  if (typeof value === 'number') {
    const b = Buffer.alloc(8);
    b.writeDoubleBE(Number.isNaN(value) ? NaN : value);
    return { type: 'number', bits: Number.isNaN(value) ? '7ff8000000000000' : b.toString('hex') };
  }
  return { type: typeof value, text: String(value) };
}
const output = {};
for (const [name, value] of Object.entries(constants)) {
  if (name === 'RECV_WINDOW') output[name] = observe(value);
  else if (Array.isArray(value)) output[name] = value;
  else if (name === 'BASE_WS_URL') output[name] = Object.entries(value).map(([key, item]) => [key, Object.entries(item)]);
  else output[name] = {
    entries: Object.entries(value),
    probes: [...Object.keys(value), '', 'unknown', 'buy', ...inherited].map(key => [key, observe(value[key])]),
  };
}
process.stdout.write(JSON.stringify(output));
