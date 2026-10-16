const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw new Error('SAFE_CEX_TYPESCRIPT_PATH must select the isolated oracle dependencies');
const axiosPath = require.resolve(path.join(path.dirname(isolated), 'axios-retry'));
const {isNetworkError} = require(axiosPath);
const allowedPath = require.resolve('is-retry-allowed', {paths:[path.dirname(axiosPath)]});
let denied;
class CaptureSet extends Set { constructor(values) {super(values); denied = [...values];} }
vm.runInNewContext(fs.readFileSync(allowedPath, 'utf8'), {Set:CaptureSet, module:{exports:{}}});
if (!denied || denied.length === 0) throw new Error('installed denylist capture failed');
const codes = [null, '', 'ERR_CANCELED', 'ECONNABORTED', 'ECONNRESET', 'ETIMEDOUT', 'ERR_NETWORK', 'UNKNOWN', ...denied];
const cases = codes.flatMap(code => [false,true].map(response => ({
  code, response, expected:isNetworkError({code, response:response ? {status:400}:undefined}),
})));
process.stdout.write(JSON.stringify({denied,cases}));
