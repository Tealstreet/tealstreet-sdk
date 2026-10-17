import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import vm from 'node:vm';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? name => require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '..', name))
  : require;
const qs = runtimeRequire('qs');
const axios = runtimeRequire('axios');
let now;
function source(relative, deps) {
  const filename = new URL('../../../packages/safe-cex/src/' + relative, import.meta.url);
  const exports = {};
  vm.runInNewContext(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {compilerOptions: {module:ts.ModuleKind.CommonJS, esModuleInterop:true}}).outputText,
    {exports, require: name => {
      if (!(name in deps)) throw Error('Unexpected dependency ' + name);
      return deps[name];
    }}, {filename: filename.pathname});
  return exports;
}
const payload = source('utils/payloadSigner.ts', {'create-hmac':runtimeRequire('create-hmac')});
const {signBinanceRequest} = source('exchanges/binance/binance.signing.ts', {
  axios, 'lodash/omit':runtimeRequire('lodash/omit'), qs,
  '../../utils/payloadSigner':payload, '../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>now}},
});
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
const results = [];
for (const vector of cases) {
  now = vector.now;
  if (vector.mode === 'bigint-query') vector.config.params.orderId = BigInt(vector.config.params.orderId);
  if (vector.mode === 'bigint-body') vector.config.data.orderId = BigInt(vector.config.data.orderId);
  const calls = [];
  const options = {...vector.options};
  if (vector.mode === 'callback' || vector.mode === 'failure') options.payloadSigner = async request => {
    calls.push(request);
    if (vector.mode === 'failure') throw Error('fixture signer refusal');
    return {signature:'fixture-signature', headers:vector.signerHeaders || {'X-Proof':'signer','X-Shared':'signer'}};
  };
  try {
    const config = await signBinanceRequest(vector.config, options, 'binance', vector.window);
    if (vector.aliasWitness) {
      const sameNested = config.params.nested === vector.config.params.nested;
      vector.config.params.nested.label = 'input-mutated';
      const fromInput = {input:vector.config.params.nested.label, output:config.params.nested.label};
      config.params.nested.label = 'output-mutated';
      const fromOutput = {input:vector.config.params.nested.label, output:config.params.nested.label};
      results.push({sameNested,fromInput,fromOutput});
      continue;
    }
    results.push({config: vector.lossy ? undefined : config, calls, query:qs.stringify(config.params,{arrayFormat:'repeat'}), inputQuery:qs.stringify(vector.config.params,{arrayFormat:'repeat'})});
  } catch(error) { results.push({error:error.message,calls}); }
}
process.stdout.write(JSON.stringify(results));
