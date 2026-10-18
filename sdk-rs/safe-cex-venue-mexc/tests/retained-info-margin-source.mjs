import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {load, root, ts} from './source.mjs';
const file = path.join(root, 'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const fields = ast.statements.filter(ts.isClassDeclaration).flatMap(c => c.members)
  .filter(m => m.name?.getText(ast) === 'deriveSymbolConfig');
if (fields.length !== 1) throw Error('deriveSymbolConfig source census');
const source = ts.transpileModule(`this.deriveSymbolConfig = ${fields[0].initializer.getText(ast)};`, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText;
const bindings = {...load('packages/safe-cex/src/types.ts'), ...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts')};
const {DefaultStore} = load('packages/safe-cex/src/store/store.base.ts');
const inputs = ['isolat', 'isolate', 'isolated', '\ud800ISOLATE', 'ISOLATED\udc00', 'isolation', '1\ud800', ''];
const cases = [...inputs.map(openType => ({openType, marginType: 'cross'})),
  {openType: undefined, marginType: 'isolated'}, {openType: null, marginType: 'isolated'}];
console.log(JSON.stringify(cases.map(({openType, marginType}) => {
  const store = new DefaultStore();
  store.update({positions: [{id: 'retained', symbol: 'BTCUSDT', side: 'long', contracts: 1, leverage: 7,
    positionMode: 'oneway', marginType, info: {openType}}]});
  const context = vm.createContext({...bindings, store, symbolSettings: new Map(), accountPositionMode: 'hedged'});
  vm.runInContext(source, context);
  return {kind: openType === undefined ? 'undefined' : openType === null ? 'null' : 'text', marginType,
    units: typeof openType === 'string' ? Array.from({length: openType.length}, (_, i) => openType.charCodeAt(i)) : [],
    expected: context.deriveSymbolConfig('BTCUSDT').marginType};
})));
