const assert = require('node:assert/strict');
const path = require('node:path');
const source = require('node:fs').readFileSync(path.resolve(__dirname, '../../../../packages/safe-cex/src/utils/axiosErrorEnhancer.ts'), 'utf8');
const signature = 'export function isNetworkError(error: any): boolean {';
assert.equal(source.split(signature).length, 2, 'live isNetworkError declaration');
const start = source.indexOf(signature);
const end = source.indexOf('\n}', start);
assert(end > start, 'live isNetworkError body');
const network = require('node:vm').runInNewContext('(' + source.slice(start, end + 2).replace(signature, 'function isNetworkError(error) {') + ')');
const raws = ['null', 'false', 'true', '0', '-0', '1', '-1', '1e400', '-1e400', '1e-400', '-1e-400', '""', '"0"', '"false"', '" "', '[]', '{}', '[null]', '"\\ufeff"', '"\\u0085"', '"😀"'];
for (let exponent = -400; exponent <= 400; exponent++) {
  raws.push(`1e${exponent}`, `-1e${exponent}`);
}
process.stdout.write(JSON.stringify(raws.map(raw => {
  const value = JSON.parse(raw);
  return { raw, truthy: Boolean(value), network: ['isAxiosError','response','code'].map(key => network({isAxiosError:true, code:'ERR_NETWORK', message:'Network Error', [key]:value})) };
})));
