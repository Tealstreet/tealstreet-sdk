// Reuse the maintained actual woo.api.ts + Axios/qs/HMAC source loader.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const moduleApi = require('node:module');
const assert = require('node:assert/strict');
const file = path.resolve(__dirname, '../scripts/signing-oracle.cjs');
const source = fs.readFileSync(file, 'utf8');
const marker = 'const cases = [';
assert.equal(source.split(marker).length, 2, 'maintained source-loader boundary');
const context = { require: moduleApi.createRequire(file), __dirname: path.dirname(file), process, Buffer, console };
vm.createContext(context);
vm.runInContext(source.split(marker)[0] + `
globalThis.observe = async function () {
  bindings['../../utils/virtualClock'].virtualClock.getCurrentTimeMs = () => 1700000000000;
  const rows = [];
  for (const [tag, number] of [['NaN', NaN], ['Infinity', Infinity]]) {
    const client = out.createAPI({ key: 'fixture-key', secret: 'fixture-secret' }, {});
    let actual;
    await client.post('/v1/client/futures_leverage', {
      symbol: 'PERP_BTC_USDT', margin_mode: 'CROSS', position_side: 'BOTH', leverage: number,
    }, { adapter: async config => {
      actual = { url: config.url, body: config.data, signature: config.headers.get('x-api-signature') };
      return { data: { success: true }, status: 200, statusText: 'OK', headers: {}, config };
    }});
    assert.equal(actual.body, 'leverage=' + tag + '&margin_mode=CROSS&position_side=BOTH&symbol=PERP_BTC_USDT');
    assert.equal(preimage, actual.body + '|1700000000000');
    rows.push({ tag, preimage, actual });
  }
  return rows;
};`, context, { filename: file });
context.observe().then(rows => process.stdout.write(JSON.stringify(rows) + '\n')).catch(error => {
  console.error(error);
  process.exitCode = 1;
});
