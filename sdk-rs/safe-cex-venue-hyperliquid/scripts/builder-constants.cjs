const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function load(name) {
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid', name), 'utf8');
  const exports = {};
  vm.runInNewContext(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS, target:ts.ScriptTarget.ES2022}}).outputText, {exports});
  return exports;
}
const core = load('hyperliquid.constants.ts'), spot = load('hyperliquidspot.constants.ts');
const units = text => Array.from({length:text.length}, (_, index) => text.charCodeAt(index));
const bits = number => {
  if (Number.isNaN(number)) return '7ff8000000000000';
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(number); return bytes.toString('hex');
};
const names = ['singularSubscriptionTypes','systemSubscriptionTypes','MAINNET_API_URL','TEALSTREET_BUILDER_ADDRESS_MAINNET','TEALSTREET_BUILDER_ADDRESS_TESTNET','TEALSTREET_BUILDER_FEE','HYPERLIQUID_BUILDER_TIERS','SPOT_ASSET_INDEX_OFFSET','PERP_MAX_PRICE_DECIMALS','SPOT_MAX_PRICE_DECIMALS'];
const constants = Object.fromEntries(names.map(name => [name, core[name]]));
const spotConstants = Object.fromEntries(['singularSubscriptionTypes','systemSubscriptionTypes','MAINNET_API_URL'].map(name => [name, spot[name]]));
const addresses = [core.TEALSTREET_BUILDER_ADDRESS_MAINNET, core.TEALSTREET_BUILDER_ADDRESS_TESTNET, ...Object.values(core.HYPERLIQUID_BUILDER_TIERS).slice(1)];
const lookups = [];
for (const address of addresses) {
  for (const value of [address, address.toUpperCase(), ` \t${address}\n`, `\ufeff${address}\u2028`, `\u0085${address}`, `${address}\u0085`, `${address}\ud800`, `\udfff${address}`]) {
    lookups.push({address:units(value), fee:core.hyperliquidBuilderFeeTenthsBps(value)});
  }
}
for (const value of ['', '0x0000000000000000000000000000000000000000', 'constructor', '__proto__', '\ud800', '雪😀']) lookups.push({address:units(value),fee:core.hyperliquidBuilderFeeTenthsBps(value)});
const known = [];
for (const address of addresses) for (const fee of [0,-0,7,10,30,50,10.5,NaN,Infinity,-Infinity]) known.push({address:units(`\ufeff${address.toUpperCase()} `),feeBits:bits(fee),value:core.isKnownHyperliquidBuilder(`\ufeff${address.toUpperCase()} `,fee)});
for (const value of ['__proto__', '\ud800', `${addresses[0]}\udfff`]) known.push({address:units(value),feeBits:bits(10),value:core.isKnownHyperliquidBuilder(value,10)});
const rates = [0,-0,1,7,10,30,50,-7,1e-9,1e308,NaN,Infinity,-Infinity].map(fee => ({feeBits:bits(fee),value:core.builderFeeRateFromTenthsBps(fee)}));
const configs = [
  undefined, {}, {testnet:false}, {testnet:true}, {testnet:true,hyperliquidBuilder:null},
  {testnet:true,hyperliquidBuilder:{address:addresses[2],feeTenthsBps:30}},
  {testnet:false,hyperliquidBuilder:{address:'custom-雪😀',feeTenthsBps:7}},
  {testnet:true,hyperliquidBuilder:{address:'\ud800custom\udfff',feeTenthsBps:-0}},
  {hyperliquidBuilder:{address:'',feeTenthsBps:NaN}},
].map(options => {
  const value = core.resolveHyperliquidBuilder(options);
  const view = config => ({address:units(config.address),feeBits:bits(config.feeTenthsBps)});
  return {options:options === undefined ? null : {testnet:options.testnet ?? null,builder:options.hyperliquidBuilder == null ? null : view(options.hyperliquidBuilder)},value:view(value),sameReference:options?.hyperliquidBuilder === value};
});
const effects = [];
for (const mode of ['builder-error','provided','net-error','fallback-mainnet','fallback-testnet']) {
  const reads = [];
  const configured = {address:addresses[2],feeTenthsBps:30};
  const options = {
    get hyperliquidBuilder() {reads.push('builder'); if(mode==='builder-error') throw new Error('builder getter failed'); return mode==='provided' ? configured : null;},
    get testnet() {reads.push('testnet'); if(mode==='net-error'||mode==='provided') throw new Error('testnet getter failed'); return mode==='fallback-testnet';},
  };
  const row = {mode,reads};
  try {
    const value = core.resolveHyperliquidBuilder(options);
    row.address = units(value.address); row.feeBits = bits(value.feeTenthsBps); row.sameReference = value===configured;
  } catch(error) {row.errorName = error.name; row.error = error.message;}
  effects.push(row);
}
assert.equal(lookups.length,38); assert.equal(known.length,43); assert.equal(rates.length,13); assert.equal(configs.length,9);
assert.equal(effects.length,5);
console.log(JSON.stringify({source:'actual core and spot leaf modules; UTF-16 native address descriptors; NaN-only canonical bits',constants,spotConstants,network:[false,true].map(testnet=>({testnet,address:core.getBuilderAddress(testnet)})),lookups,known,rates,configs,effects}));
