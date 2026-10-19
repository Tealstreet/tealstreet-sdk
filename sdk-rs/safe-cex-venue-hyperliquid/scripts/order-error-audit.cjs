// Fresh maintained source, including the actual class consumer's catch branch.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = (file) => fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid', file), 'utf8');
const options = { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } };
const exportsObject = {};
const context = vm.createContext({ exports: exportsObject });
vm.runInContext(ts.transpileModule(source('hyperliquid.order-response.ts'), options).outputText, context);
const file = ts.createSourceFile('hyperliquid.exchange.ts', source('hyperliquid.exchange.ts'), ts.ScriptTarget.Latest, true);
const classes = file.statements.filter(ts.isClassDeclaration);
const methods = classes.flatMap((node) => node.members).filter((node) => ts.isMethodDeclaration(node)
  && node.name.getText(file) === 'validateResponseAndExtractOrderIds');
assert.equal(methods.length, 1, 'exact maintained placement response consumer');
context.parseHyperliquidOrderResponse = exportsObject.parseHyperliquidOrderResponse;
context.HyperliquidUnreadableOrderResponseError = exportsObject.HyperliquidUnreadableOrderResponseError;
vm.runInContext(ts.transpileModule(`class Consumer {
  refreshes = 0;
  scheduleOrderRefresh() { this.refreshes++; }
  ${methods[0].getText(file)}
}
globalThis.consumer = new Consumer();`, options).outputText, context);
const units = (value) => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
context.profile = (error) => {
  const C = exportsObject.HyperliquidUnreadableOrderResponseError;
  const E = vm.runInContext('Error', context);
  const descriptor = (key) => {
    const d = Object.getOwnPropertyDescriptor(error, key);
    return d ? [d.enumerable, d.writable, d.configurable] : null;
  };
  return {
    name: units(error.name), message: units(error.message), display: units(String(error)),
    isError: error instanceof E, isUnreadable: error instanceof C,
    directPrototype: Object.getPrototypeOf(error) === C.prototype,
    constructor: error.constructor === C,
    prototypeParent: Object.getPrototypeOf(C.prototype) === E.prototype,
    constructorParent: Object.getPrototypeOf(C) === E,
    nameDescriptor: descriptor('name'), messageDescriptor: descriptor('message'),
    enumerableKeys: Object.keys(error),
  };
};
const constructors = ['', 'unreadable order', '\ud83d\ude80', '\ud800x'].map((message) => ({
  message: units(message), observed: context.profile(new exportsObject.HyperliquidUnreadableOrderResponseError(message)),
}));
const cases = [
  { undefined: true }, { input: null }, { input: {} }, { input: { statuses: [] } },
  { input: { statuses: [null] } }, { input: { statuses: [7] } },
  { input: { statuses: ['unknown'] } }, { input: { statuses: [{}] } },
  { input: { statuses: [{ resting: {} }] } },
  { input: { statuses: [{ error: 'venue refusal' }] } },
  { input: { status: 'err', response: 'venue refusal' } },
  { input: { response: { data: { statuses: [{ resting: { oid: 123 } }, { filled: { oid: 456 } }] } } } },
  { input: { statuses: ['waitingForTrigger'] } }, { input: { statuses: ['waitingForFill'] } },
].map((row) => {
  const before = context.consumer.refreshes;
  // Parse inside the same VM so the source's instanceof and intrinsics are live.
  context.inputText = JSON.stringify(row.input);
  try {
    row.ids = vm.runInContext('consumer.validateResponseAndExtractOrderIds(inputText === undefined ? undefined : JSON.parse(inputText))', context);
  } catch (error) { row.error = context.profile(error); }
  row.refreshes = context.consumer.refreshes - before;
  return row;
});
assert.equal(constructors.length, 4);
assert.equal(cases.length, 14);
assert.equal(cases.filter((row) => row.error?.isUnreadable).length, 9);
assert.equal(cases.filter((row) => row.error && !row.error.isUnreadable).length, 2);
assert.equal(cases.filter((row) => row.ids).length, 3);
console.log(JSON.stringify({ constructors, cases }));
