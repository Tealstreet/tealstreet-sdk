const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(file) {
  if(cache.has(file)) return cache.get(file);
  assert.ok(file.startsWith(path.join(root,'packages/safe-cex/src/')), file);
  const exports={}; cache.set(file,exports);
  let source=fs.readFileSync(file,'utf8');
  if (file === path.join(root, 'packages/safe-cex/src/types.ts')) {
    const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
    source=ast.statements.filter(ts.isEnumDeclaration).map(s=>s.getText(ast)).join('\n');
  }
  const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
  vm.runInNewContext(js,{exports,require:(name)=>{
    assert.ok(name.startsWith('.'),`unmocked runtime import ${name}`);
    return load(path.resolve(path.dirname(file),name)+'.ts');
  }},{filename:file});
  return exports;
}
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/woo/woo.types.ts'));
const ordered=Object.fromEntries(Object.entries(constants).map(([k,v])=>[k,Array.isArray(v)||typeof v!=='object'?v:Object.entries(v).map(([key,value])=>[key,typeof value==='object'?Object.entries(value):value])]));
const prototypeLookups=Object.fromEntries(['ORDER_TYPE','ORDER_SIDE','ORDER_TRIGGER_TYPE'].map(name=>[name,Object.fromEntries(['constructor','__proto__','toString','valueOf','hasOwnProperty'].map(key=>[key,typeof constants[name][key]]))]));
const target=path.join(__dirname,'../tests/fixtures/constants.json');
const bytes=JSON.stringify({ordered,prototypeLookups},null,2)+'\n';
if(process.argv.includes('--check')) assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO live constants/prototype oracle drift');
else fs.writeFileSync(target,bytes);
console.log(`WOO live constants oracle: ${Object.keys(constants).length} tables/scalars`);
