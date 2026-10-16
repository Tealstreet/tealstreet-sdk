// Reachability inventory, not a direct-write grep. Review unresolved property
// calls as native/data operations; arbitrary host callback effects are external.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const roots=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const files=[...new Set(roots.map(r=>r.file))], functions=new Map();
for(const file of files){const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);function visit(n){if(ts.isFunctionDeclaration(n)&&n.name&&n.body)functions.set(file+'::'+n.name.text,{ast,node:n.body,name:n.name.text,file});if(ts.isVariableDeclaration(n)&&n.initializer&&(ts.isArrowFunction(n.initializer)||ts.isFunctionExpression(n.initializer)))functions.set(file+'::'+n.name.text,{ast,node:n.initializer.body,name:n.name.text,file});ts.forEachChild(n,visit);}visit(ast);}
const seen=new Set(), report=[];
function walk(key){if(seen.has(key))return;seen.add(key);const f=functions.get(key);assert.ok(f,'all roots/callees must resolve '+key);const calls=[];function visit(n){if(ts.isCallExpression(n)){const text=n.expression.getText(f.ast);calls.push(text);if(ts.isIdentifier(n.expression)&&functions.has(f.file+'::'+text))walk(f.file+'::'+text);}ts.forEachChild(n,visit);}visit(f.node);report.push({key,calls:[...new Set(calls)].sort()});}
for(const root of roots)walk(root.file+'::'+root.name);
const result={rootCount:roots.length,reachableLocalFunctions:seen.size,functions:report.sort((a,b)=>a.key.localeCompare(b.key))};
process.stdout.write(JSON.stringify(result,null,2)+'\n');
