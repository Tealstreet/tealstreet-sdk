// Syntactic routing census. These buckets are NOT validated cost populations.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const rows=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(rows.length,7341,'independent manifest inventory floor');
const trees=new Map();
const resolvePrivate=process.argv.includes('--resolve-private');
function index(file){
 if(trees.has(file))return trees.get(file);
 const text=fs.readFileSync(path.join(root,file),'utf8'),tree=ts.createSourceFile(file,text,99,true),found=new Map();
 assert.equal(tree.parseDiagnostics.length,0,`parse ${file}`);
 const put=(key,node)=>{const a=found.get(key)||[];a.push(node);found.set(key,a);};
 function visit(node,owner=''){
  if(ts.isClassDeclaration(node)&&node.name){put(node.name.text,node);for(const member of node.members){
   let name=member.name?.getText(tree);if(ts.isConstructorDeclaration(member))name='constructor';
   if(ts.isGetAccessor(member))name='get '+name;if(ts.isSetAccessor(member))name='set '+name;
   if(name){
    const modifiers=ts.canHaveModifiers(member)?ts.getModifiers(member):undefined;
    const stat=modifiers?.some(m=>m.kind===ts.SyntaxKind.StaticKeyword);
    const accessor=ts.isGetAccessor(member)||ts.isSetAccessor(member);
    const priv=resolvePrivate&&!stat&&!accessor&&member.name&&!ts.isPrivateIdentifier(member.name)&&modifiers?.some(m=>m.kind===ts.SyntaxKind.PrivateKeyword);
    put(node.name.text+(priv?'#':'.')+(resolvePrivate&&stat?'static ':'')+name,member);
   }
  }return;}
  if((ts.isFunctionDeclaration(node)||ts.isInterfaceDeclaration(node)||ts.isTypeAliasDeclaration(node)||ts.isEnumDeclaration(node))&&node.name)put(node.name.text,node);
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name))put(node.name.text,node);
  ts.forEachChild(node,child=>visit(child,owner));
 }
 visit(tree);trees.set(file,{tree,found});return trees.get(file);
}
function literal(n){if(!n)return false;while(ts.isAsExpression(n)||ts.isParenthesizedExpression(n)||ts.isSatisfiesExpression(n))n=n.expression;return ts.isStringLiteral(n)||ts.isNumericLiteral(n)||[ts.SyntaxKind.TrueKeyword,ts.SyntaxKind.FalseKeyword,ts.SyntaxKind.NullKeyword].includes(n.kind)||(ts.isPrefixUnaryExpression(n)&&literal(n.operand));}
function primitive(n){if(!n)return false;return [ts.SyntaxKind.StringKeyword,ts.SyntaxKind.NumberKeyword,ts.SyntaxKind.BooleanKeyword].includes(n.kind);}
function classify(row){
 const {tree,found}=index(row.source),matches=found.get(row.ts)||[];
 if(matches.length!==1)return{class:'unpriced_resolution',reason:matches.length?'ambiguous syntactic node':'no unique syntactic declaration'};
 const declaration=matches[0];
 if(row.kind==='class'||row.kind==='constructor')return{class:'unpriced_structure',reason:'class/constructor composition not sampled'};
 if(row.kind==='type')return{class:ts.isInterfaceDeclaration(declaration)&&!declaration.heritageClauses&&declaration.members.length>0&&declaration.members.every(m=>ts.isPropertySignature(m)&&!m.questionToken&&primitive(m.type))?'record':'unpriced_type',reason:'direct required primitive-field interface only; no checker/accepted-set claim'};
 if(row.kind==='enum')return{class:'unpriced_enum',reason:'enum protocols not sampled'};
 let callable=declaration;
 if(ts.isVariableDeclaration(declaration)||ts.isPropertyDeclaration(declaration)){
  let init=declaration.initializer;while(init&&(ts.isAsExpression(init)||ts.isParenthesizedExpression(init)||ts.isSatisfiesExpression(init)))init=init.expression;
  if(init&&(ts.isArrowFunction(init)||ts.isFunctionExpression(init)))callable=init;
  else return{class:literal(init)?'closed':'unpriced_binding',reason:'literal scalar initializer or unpriced table/import/call binding'};
 }
 if(!callable.body)return{class:'unpriced_callable',reason:'no executable body resolved'};
 const body=callable.body,stats={awaits:0,receiver:false,loops:0,collection:false,unknownCalls:[]};
 function scan(node){
  if(node.kind===ts.SyntaxKind.ThisKeyword)stats.receiver=true;
  if(ts.isAwaitExpression(node))stats.awaits++;
  if(ts.isForStatement(node)||ts.isForOfStatement(node)||ts.isForInStatement(node)||ts.isWhileStatement(node)||ts.isDoStatement(node))stats.loops++;
  if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&['map','flatMap','filter','reduce','forEach','parse'].includes(node.expression.name.text))stats.collection=true;
  if(ts.isNewExpression(node)&&!['Error','TypeError','RangeError'].includes(node.expression.getText(tree)))stats.unknownCalls.push('new '+node.expression.getText(tree));
  if(ts.isCallExpression(node)){const name=node.expression.getText(tree);if(!/^(Number|String|Boolean|parseFloat|parseInt|isNaN|isFinite)$/.test(name)&&!/^((Math|Number|Array)\.[a-zA-Z]+|JSON\.parse)$/.test(name)&&! /\.(toFixed|replace|indexOf|map|flatMap|filter|reduce|forEach)$/.test(name)&&name!=='SUPPORTED_TICK_SIZES.has')stats.unknownCalls.push(name);}
  ts.forEachChild(node,scan);
 }scan(body);
 const async=!!callable.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword);
 const expression=ts.isBlock(body)?body.statements.length===1&&ts.isReturnStatement(body.statements[0])?body.statements[0].expression:null:body;
 let route;
 if(!async&&literal(expression))route='closed';
 else if(!async&&!stats.receiver&&!stats.awaits)route=stats.unknownCalls.length?'unpriced_external_calls':stats.loops||stats.collection?'collection':'scalar';
 else if(async&&stats.receiver&&stats.awaits===1&&!stats.loops&&/\bthis\.xhr\./.test(body.getText(tree))&&ts.isBlock(body)&&body.statements.length<=3)route='http';
 else if(async&&stats.receiver)route='orchestration';
 else if(resolvePrivate&&!async&&stats.receiver)route='receiver_sync';
 else route='unpriced_callable';
 return{class:route,reason:'syntactic routing only',stats};
}
const inventory=rows.map(row=>({...row,...classify(row)}));
const counts={};for(const row of inventory)counts[row.class]=(counts[row.class]||0)+1;
assert.equal(Object.values(counts).reduce((a,b)=>a+b,0),7341,'disjoint census partition');
process.stdout.write(JSON.stringify({denominator:7341,counts,inventory},null,2)+'\n');
