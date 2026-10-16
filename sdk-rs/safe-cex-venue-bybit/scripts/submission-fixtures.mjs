import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const source = ts.createSourceFile('bybit.ts', readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts', import.meta.url), 'utf8'), ts.ScriptTarget.Latest, true);
const owner = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
const method = owner.members.find(n => n.name?.getText(source) === 'submitCreateOrders');
assert.ok(method?.body, 'live submitCreateOrders required');
const context = vm.createContext({
  ENDPOINTS: { CREATE_ORDER: '/v5/order/create' },
  mapSeries: async (items, callback) => { const results = []; for (const item of items) results.push(await callback(item)); return results; },
});
vm.runInContext(ts.transpileModule(`class Submission { ${method.getText(source)} }; globalThis.subject = new Submission();`, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText, context);
const cases = [];
for (let count = 0; count <= 6; count++) for (let mask = 0; mask < 2 ** count; mask++) {
  const payloads = Array.from({length: count}, (_, i) => ({symbol: `S${i}`, orderLinkId: String(i)}));
  const calls = [], reports = [], sequence = [];
  context.subject.xhr = {post: async (url, payload) => {
    const i = calls.length; calls.push({url, payload}); sequence.push(`post:${i}`);
    await Promise.resolve(); sequence.push(`settled:${i}`);
    if (mask & (1 << i)) throw {response: {data: {retMsg: `venue:${i}`}}, message: `transport:${i}`};
    return {data: {result: {orderId: String(i)}, retCode: i}};
  }};
  context.subject.formatBybitVenueError = (body, message) => { reports.push({body,message}); return `reported:${body.retMsg}:${message}`; };
  context.subject.emitAndLogError = message => sequence.push(message);
  const result = await context.subject.submitCreateOrders(payloads, {});
  cases.push({payloads, mask, calls, reports, sequence, result});
}
assert.equal(cases.length, 127);
process.stdout.write(JSON.stringify(cases));
