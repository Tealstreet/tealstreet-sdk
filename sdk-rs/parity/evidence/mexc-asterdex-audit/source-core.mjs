// Source-only observations. Production/native code is never mutated by this probe.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler environment required');
export const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
export const runtimeRequire=createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH),'audit-oracle.cjs'));
export const root=fileURLToPath(new URL('../../../../',import.meta.url));

export function once(text,before,after){assert.equal(text.split(before).length-1,1,'AUDIT source mutation selector');return text.replace(before,after);}
export function declaration(source,name,owner){
 const ast=ts.createSourceFile('actual.ts',source,ts.ScriptTarget.Latest,true);
 const entries=owner?ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===owner).members:ast.statements;
 const found=[...entries].filter(n=>owner?n.name?.getText(ast)===name:ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));
 assert.equal(found.length,1,'actual declaration '+name);return found[0].getText(ast);
}
export function execute(source,bindings={}){
 const result=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true},reportDiagnostics:true});
 assert.equal(result.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 const exports={};vm.runInNewContext(result.outputText,{exports,...bindings});return exports;
}
export function loader(transform=text=>text,boundaries=new Map()){
 const cache=new Map();
 return function load(relative){
  const file=path.resolve(root,relative);
  if(boundaries.has(file))return boundaries.get(file);
  if(cache.has(file))return cache.get(file).exports;
  const module={exports:{}};cache.set(file,module);
  const source=transform(fs.readFileSync(file,'utf8'),file);
  const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  vm.runInThisContext('(function(require,module,exports){'+js+'\n})',{filename:'actual-venue-source.ts'})(specifier=>{
   if(boundaries.has(specifier))return boundaries.get(specifier);
   if(specifier.startsWith('@tealstreet/'))return load('packages/'+specifier.slice('@tealstreet/'.length)+'.ts');
   if(!specifier.startsWith('.'))return runtimeRequire(specifier);
   const target=path.resolve(path.dirname(file),specifier);return load(target.endsWith('.ts')?target:target+'.ts');
  },module,module.exports);return module.exports;
 };
}
