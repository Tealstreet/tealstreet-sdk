// Derive direct venue targets from live TS import edges and per-source manifest ownership.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root = fileURLToPath(new URL('../../', import.meta.url));
function files(dir) { return fs.readdirSync(dir,{withFileTypes:true}).flatMap(e=>e.isDirectory()?files(path.join(dir,e.name)):[path.join(dir,e.name)]); }
export function measureVenueDependencies() {
  const owners = new Map();
  for (const file of files(path.join(root,'sdk-rs/parity/manifest')).filter(p=>p.endsWith('.toml'))) {
    const header = fs.readFileSync(file,'utf8').split('[[symbol]]')[0];
    const source = JSON.parse(header.match(/^ts\s*=\s*(".*")$/m)[1]);
    const declaration = header.match(/^rust_files\s*=\s*(\[[\s\S]*?\])/m);
    const rust = declaration ? JSON.parse(declaration[1]) : [];
    owners.set(source,[...new Set(rust.map(p=>p.split('/')[0]))]);
  }
  const result = {};
  const resolve = (source, specifier) => {
    let base;
    if (specifier.startsWith('.')) base=path.posix.normalize(path.posix.join(path.posix.dirname(source),specifier));
    else if (specifier.startsWith('@tealstreet/')) base='packages/'+specifier.slice('@tealstreet/'.length);
    else return undefined;
    return [base,base+'.ts',base+'.tsx',base+'/index.ts',base+'/src/index.ts'].find(p=>fs.existsSync(path.join(root,p)) && fs.statSync(path.join(root,p)).isFile());
  };
  for (const source of [...owners.keys()].sort()) {
    const venue = source.match(/^packages\/safe-cex\/src\/exchanges\/([^/]+)\//)?.[1];
    if (!venue) continue;
    const entry = result[venue] ??= { targets:{}, unowned:[] };
    const text=fs.readFileSync(path.join(root,source),'utf8');
    const ast=ts.createSourceFile(source,text,ts.ScriptTarget.Latest,true);
    const imports=[];
    const visit=node=>{
      if ((ts.isImportDeclaration(node)||ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) imports.push(node.moduleSpecifier.text);
      if (ts.isCallExpression(node) && node.expression.kind===ts.SyntaxKind.ImportKeyword && node.arguments[0] && ts.isStringLiteral(node.arguments[0])) imports.push(node.arguments[0].text);
      ts.forEachChild(node,visit);
    };visit(ast);
    for (const specifier of imports) {
      const imported=resolve(source,specifier);
      if (!imported || imported.startsWith(`packages/safe-cex/src/exchanges/${venue}/`)) continue;
      const crates=owners.get(imported) ?? [];
      if (!crates.length) entry.unowned.push({source,imported});
      for (const crate of crates) {
        if (crate===`safe-cex-venue-${venue}`) continue;
        (entry.targets[crate] ??= []).push({source,imported});
      }
    }
  }
  for (const entry of Object.values(result)) entry.targets=Object.fromEntries(Object.entries(entry.targets).sort(([a],[b])=>a.localeCompare(b)));
  assert.equal(Object.keys(result).length,26,'expected one source family per venue crate');
  return result;
}
if (process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) console.log(JSON.stringify(measureVenueDependencies(),null,2));
