const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-public-frames.ts');
const source = fs.readFileSync(file, 'utf8');
const exportsObject = {};
const protocol = { CoinbaseStarbaseProtocolError: class CoinbaseStarbaseProtocolError extends Error {} };
const compiled = ts.transpileModule(source + '\nexport { isSupportedPublicChannel, isRecord, hasExactKeys };', {
  compilerOptions: { module: ts.ModuleKind.CommonJS },
}).outputText;
vm.runInNewContext(compiled, { exports: exportsObject, require: () => protocol });

const instruments = ['BTC_USDC-PERPETUAL', 'ETH_USDC-PERPETUAL'];
const intervals = ['100ms', 'agg2'];
const resolutions = ['1', '3', '5', '15', '30', '60', '120', '360', '720', '1D'];
const candidates = new Set([
  '', 'ticker', 'book', 'chart.trades', 'instrument.state.future', 'constructor', '__proto__',
  'ticker.SOL_USDC-PERPETUAL.100ms', 'ticker.BTC_USDC-PERPETUAL.200ms',
  'book.ETH_USDC-PERPETUAL.agg3', 'chart.trades.BTC_USDC-PERPETUAL.2',
  'instrument.state.future.toString', 'instrument.state.future.BTC.extra',
  'ticker. BTC_USDC-PERPETUAL.100ms', 'ticker.BTC_USDC-PERPETUAL.\uFEFF100ms',
  'ticker.BTC_USDC-PERPETUAL.100ms\u0085',
]);

for (const instrument of instruments) {
  for (const category of ['ticker', 'book']) {
    for (const interval of intervals) candidates.add(`${category}.${instrument}.${interval}`);
  }
  for (const resolution of resolutions) candidates.add(`chart.trades.${instrument}.${resolution}`);
}
for (const currency of ['BTC', 'ETH', 'USDC', 'any']) candidates.add(`instrument.state.future.${currency}`);

const channelCases = [...candidates].map((input) => ({
  input,
  result: exportsObject.isSupportedPublicChannel(input),
}));
const keyCases = [
  { label: 'ordinary', value: { a: 1, b: 2 }, expected: ['a', 'b'] },
  { label: 'different-order', value: { b: 2, a: 1 }, expected: ['a', 'b'] },
  { label: 'numeric-index-keys', value: JSON.parse('{"10":"ten","2":"two","name":"value"}'), expected: ['name', '2', '10'] },
  { label: 'prototype-named-own-keys', value: JSON.parse('{"__proto__":"own","constructor":"own"}'), expected: ['__proto__', 'constructor'] },
  { label: 'extra-key', value: { a: 1, b: 2, c: 3 }, expected: ['a', 'b'] },
  { label: 'missing-key', value: { a: 1 }, expected: ['a', 'b'] },
  { label: 'wrong-same-size', value: { a: 1, c: 3 }, expected: ['a', 'b'] },
].map(({ label, value, expected }) => ({
  label,
  value,
  expected,
  result: exportsObject.hasExactKeys(value, expected),
}));
const recordValues = [undefined, null, false, true, 0, '', 'x', [], [1], {}, { x: 1 }];
const recordCases = recordValues.map((input) => ({
  kind: 'record',
  ...(input === undefined ? {} : { input }),
  result: exportsObject.isRecord(input),
}));

process.stdout.write(JSON.stringify(channelCases.concat(keyCases, recordCases)));
