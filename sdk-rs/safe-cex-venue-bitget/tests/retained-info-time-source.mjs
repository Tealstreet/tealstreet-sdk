import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = new URL('../../../packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts', import.meta.url);
const ast = ts.createSourceFile(file.pathname, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['toFiniteNumber', 'getPositionPayloadUpdateTime'];
const fields = ast.statements.filter(ts.isClassDeclaration).flatMap(c => c.members)
  .filter(m => names.includes(m.name?.getText(ast)))
  .map(m => `this.${m.name.getText(ast)} = ${m.initializer.getText(ast)};`);
if (fields.length !== 2) throw Error('position time source census');
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(fields.join('\n'), {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText, context);
const inputs = [
  {uTime: '1700000000010\ud800'}, {uTime: '\ud8001700000000010'},
  {uTime: undefined, updatedTime: 23}, {uTime: null, updatedTime: 24},
  {uTime: NaN, updatedTime: 25}, {uTime: Infinity}, {uTime: -0},
  {uTime: ''}, {uTime: ' \ufeff12.5tail\udc00'},
];
const bits = n => {const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');};
const encode = v => v === undefined ? {kind: 'undefined'} : v === null ? {kind: 'null'} :
  typeof v === 'number' ? {kind: 'number', bits: bits(v)} : {kind: 'text', units: Array.from({length: v.length}, (_, i) => v.charCodeAt(i))};
console.log(JSON.stringify(inputs.map(info => {
  const result = context.getPositionPayloadUpdateTime({info});
  return {fields: Object.entries(info).map(([key, value]) => [key, encode(value)]), expected: result === undefined ? null : bits(result)};
})));
