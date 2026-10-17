import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';

const filename = fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts', import.meta.url));
const source = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BingxExchange');
assert(klass);
const members = ['fetchHistoricalFills', 'toBingxSymbol'].map(name => {
  const matches = klass.members.filter(node => node.name?.getText(source) === name);
  assert.equal(matches.length, 1, `${name} source census`);
  return matches[0].getText(source);
});
function declaration(relative, name) {
  const filename = fileURLToPath(new URL(relative, import.meta.url));
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const matches = ast.statements.filter(node => node.name?.text === name ||
    (ts.isVariableStatement(node) && node.declarationList.declarations.some(value => value.name.getText(ast) === name)));
  assert.equal(matches.length, 1, `${name} declaration census`);
  return matches[0].getText(ast).replace(/^export /, '');
}
const dependencies = [
  declaration('../../../packages/safe-cex/src/types.ts', 'OrderSide'),
  declaration('../../../packages/safe-cex/src/exchanges/bingx/bingx.types.ts', 'ENDPOINTS'),
  declaration('../../../packages/logging/src/LogSeverity.ts', 'LogSeverity'),
].join('\n');
const code = ts.transpileModule(`${dependencies} class Exchange { ${members.join('\n')} }; Exchange`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const Exchange = vm.runInNewContext(code);
const localDates = ['2024-01-15T12:34:56.789', '2024-07-15T12:34:56.789'];
const times = [...localDates, ...localDates.map(value => value + 'Z'), ...localDates.map(value => value + '+08:00')];
const fills = times.map((filledTime, i) => ({
  filledTime, orderId: String(i), symbol: 'BTC-USDT', side: 'BUY', price: '12.5', volume: '2', commission: '-0.25',
}));
const receiver = new Exchange();
const requests = [];
receiver.store = {marketsBySymbol: {BTCUSDT: {id: 'BTC-USDT'}}};
receiver.isPublicDataOnly = false;
receiver.log = () => {};
receiver.xhr = {get: async (url, config) => {
  requests.push({url, config});
  return {data: {code: 0, data: {fill_history_orders: fills}}};
}};
const executions = await receiver.fetchHistoricalFills({symbol: 'BTCUSDT'});
assert.equal(executions.length, times.length);
assert.equal(requests.length, 1);
assert.equal(requests[0].url, '/openApi/swap/v2/trade/fillHistory');
assert.equal(Intl.DateTimeFormat().resolvedOptions().timeZone, process.env.TZ);
const conversions = localDates.map(value => ({wall: Date.parse(value + 'Z'), utc: Date.parse(value)}));
assert.deepEqual(conversions.map(({wall, utc}) => (utc - wall) / 3_600_000),
  process.env.TZ === 'America/New_York' ? [5, 4] : [-8, -8]);
process.stdout.write(JSON.stringify({zone: process.env.TZ, fills, executions, conversions}));
