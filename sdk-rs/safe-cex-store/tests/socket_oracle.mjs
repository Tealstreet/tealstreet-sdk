import assert from 'node:assert/strict';
import fs from 'node:fs';
import {ts} from './source_loader.mjs';
import vm from 'node:vm';
// Execute the live standalone function while avoiding unrelated exchange imports.
const sourcePath = new URL('../../../packages/safe-cex/src/exchanges/base.ts', import.meta.url);
const source = fs.readFileSync(sourcePath, 'utf8');
const ast = ts.createSourceFile('base.ts',source,ts.ScriptTarget.Latest,true);
const declaration = ast.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === 'summarizeSocketDiagnostics'));
if (!declaration) throw new Error('summarizeSocketDiagnostics declaration missing');
const body = declaration.getText(ast).replace(/^export\s+/, '');
const code = ts.transpileModule(body+'\nglobalThis.summarize = summarizeSocketDiagnostics;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const context = vm.createContext({});
vm.runInContext(code,context);
const inputs = [[],[null],[{}],[{lastCloseCode:1000}],[{lastCloseCode:1000,lastCloseAt:0},{lastCloseCode:1006,lastCloseAt:0}],[{lastCloseCode:1006,lastCloseAt:20,lifetimeCloseCount:150,onCloseCount:0},{lastCloseCode:4001,lastCloseAt:10,lifetimeCloseCount:5,onCloseCount:3,isReconnectPauseActive:true}],[null,{lastCloseAt:999,lifetimeCloseCount:4},{lastCloseCode:1001,lastCloseAt:-1}], [{lifetimeCloseCount:0.1,onCloseCount:0.1},{lifetimeCloseCount:0.2,onCloseCount:0.2}]];
const cases = [];
for (const kind of ['public','private']) for (const sockets of inputs) cases.push({kind,sockets,expected:context.summarize(sockets,kind)});
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(cases.length, 16);
const path = new URL('./socket_oracle.json',import.meta.url);
const output = JSON.stringify(cases,null,2)+'\n';
if (process.argv.includes('--check')) {
  if (fs.readFileSync(path,'utf8') !== output) throw new Error('socket oracle fixtures changed');
  console.log(`${cases.length} socket source-oracle snapshots verified`);
} else fs.writeFileSync(path,output);
