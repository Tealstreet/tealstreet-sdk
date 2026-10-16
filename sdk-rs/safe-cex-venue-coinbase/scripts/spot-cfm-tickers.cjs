const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const exchangePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.exchange.ts');
const exchangeText = fs.readFileSync(exchangePath, 'utf8');
const exchangeSource = ts.createSourceFile(exchangePath, exchangeText, ts.ScriptTarget.Latest, true);
const exchange = exchangeSource.statements.find((statement) =>
  ts.isClassDeclaration(statement) && statement.name?.text === 'CoinbaseExchange',
);
assert.ok(exchange, 'CoinbaseExchange class must exist');
const formatter = exchange.members.find((member) =>
  ts.isPropertyDeclaration(member) && member.name?.getText(exchangeSource) === 'formatSpotCfmTicker',
);
assert.ok(formatter?.initializer, 'formatSpotCfmTicker declaration must exist');

const mathPath = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/safe-math.ts');
const mathSource = ts.createSourceFile(mathPath, fs.readFileSync(mathPath, 'utf8'), ts.ScriptTarget.Latest, true);
const round = mathSource.statements.find((statement) =>
  ts.isFunctionDeclaration(statement) && statement.name?.text === 'round',
);
assert.ok(round, 'safe-math round must exist');
const isNotNumberPath = path.resolve(__dirname, '../../../packages/math/src/isNotNumber.ts');
const numberSource = ts.createSourceFile(isNotNumberPath, fs.readFileSync(isNotNumberPath, 'utf8'), ts.ScriptTarget.Latest, true);
const isNotNumber = numberSource.statements
  .filter(ts.isVariableStatement)
  .flatMap((statement) => Array.from(statement.declarationList.declarations))
  .find((declaration) => declaration.name.getText(numberSource) === 'isNotNumber');
assert.ok(isNotNumber?.initializer, 'shared isNotNumber declaration must exist');

const exportsObject = {};
const executable = [
  `const isNotNumber = ${isNotNumber.initializer.getText(numberSource)};`,
  round.getText(mathSource),
  `const formatSpotCfmTicker = ${formatter.initializer.getText(exchangeSource)};`,
  'export { formatSpotCfmTicker };',
].join('\n');
const compiled = ts.transpileModule(executable, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
vm.runInNewContext(compiled, { exports: exportsObject });

const products = [];
for (let index = 0; index < 20; index += 1) {
  const n = index + 1;
  products.push({
    product_id: `PRODUCT-${n}`,
    price: `${n + 0.234567}`,
    price_percentage_change_24h: `${n * 0.012345}`,
    volume_24h: `${n * 1.98765}`,
    future_product_details: {
      contract_size: `${(n * 0.031415).toPrecision(7)}`,
      perpetual_details: { open_interest: `${(n * 0.271828).toPrecision(8)}` },
    },
  });
}
const cases = products.map((product, index) => ({ label: `distinct-${index}`, products: [product] }));
cases.push({
  label: 'two-products',
  products: [
    { product_id: 'LEFT', price: '2.3456', price_percentage_change_24h: '7.8912', volume_24h: '3.4567', future_product_details: { contract_size: '0.125', perpetual_details: { open_interest: '11.111' } } },
    { product_id: 'RIGHT', price: '19.8765', price_percentage_change_24h: '0.0123', volume_24h: '23.4567', future_product_details: { contract_size: '2.75', perpetual_details: { open_interest: '0.9876' } } },
  ],
});
cases.push({ label: 'empty-products', products: [] });
cases.push({
  label: 'empty-string-coercions',
  products: [{ product_id: 'EMPTY', price: '', price_percentage_change_24h: '', volume_24h: '', future_product_details: { contract_size: '0', perpetual_details: { open_interest: '' } } }],
});
cases.push({
  label: 'negative-zero-price',
  products: [{ product_id: 'NEGATIVE-ZERO', price: '-0', price_percentage_change_24h: '0.1234', volume_24h: '2.3456', future_product_details: { contract_size: '3.4567', perpetual_details: { open_interest: '4.5678' } } }],
});
cases.push({
  label: 'missing-and-prefix-numbers',
  products: [
    { product_id: 'MISSING', future_product_details: { contract_size: '', perpetual_details: {} } },
    { product_id: 'PREFIX', price: '12.5tail', price_percentage_change_24h: '0x10', volume_24h: 'Infinity', future_product_details: { contract_size: '2abc', perpetual_details: { open_interest: '3e2tail' } } },
  ],
});

function taggedNumber(value) {
  if (Number.isNaN(value)) return { $number: 'NaN' };
  if (value === Infinity) return { $number: 'Infinity' };
  if (value === -Infinity) return { $number: '-Infinity' };
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value, false);
  return { $f64bits: view.getBigUint64(0, false).toString(16).padStart(16, '0') };
}

function captureTicker(ticker) {
  return {
    id: ticker.id,
    symbol: ticker.symbol,
    bid: taggedNumber(ticker.bid),
    ask: taggedNumber(ticker.ask),
    last: taggedNumber(ticker.last),
    mark: taggedNumber(ticker.mark),
    index: taggedNumber(ticker.index),
    percentage: taggedNumber(ticker.percentage),
    fundingRate: taggedNumber(ticker.fundingRate),
    fundingTime: taggedNumber(ticker.fundingTime),
    volume: taggedNumber(ticker.volume),
    quoteVolume: taggedNumber(ticker.quoteVolume),
    openInterest: taggedNumber(ticker.openInterest),
  };
}

const output = cases.map(({ label, products: caseProducts }) => ({
  label,
  expectedCount: caseProducts.length,
  input: { products: caseProducts },
  result: exportsObject.formatSpotCfmTicker({ products: caseProducts }).map(captureTicker),
}));
process.stdout.write(JSON.stringify({
  captureArchitecture: process.arch,
  captureNodeMajor: Number(process.versions.node.split('.')[0]),
  cases: output,
}));
