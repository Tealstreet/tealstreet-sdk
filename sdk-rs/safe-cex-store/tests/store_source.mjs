import fs from 'node:fs';
import {ts} from './source_loader.mjs';
import vm from 'node:vm';
export function sourceStore(mutateSource = source => source) {
  const source = mutateSource(fs.readFileSync(new URL('../../../packages/safe-cex/src/store/store.base.ts',import.meta.url),'utf8'));
  const ast = ts.createSourceFile('store.ts',source,ts.ScriptTarget.Latest,true);
  const body = ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n');
  const context = vm.createContext({clone:structuredClone, uniqBy:items=>items, OrderStatus:{Closed:'closed',Canceled:'canceled'}, virtualClock:{getCurrentTimeMs:()=>context.now},now:0,console});
  vm.runInContext(ts.transpileModule(body.replace(/\bexport /g,'')+'\nglobalThis.Store = DefaultStore;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
  return context;
}
export function requestRecorder() {
  const source=fs.readFileSync(new URL('../../../packages/safe-cex/src/safeCexAxiosCreate.ts',import.meta.url),'utf8');
  const ast=ts.createSourceFile('axios.ts',source,ts.ScriptTarget.Latest,true);
  const decl=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='recordRequestOutcome'));
  const context=vm.createContext({isSigningCaptureRequest:config=>config?.capture===true});
  vm.runInContext(ts.transpileModule(decl.getText(ast)+'\nglobalThis.record=recordRequestOutcome;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
  return context.record;
}
