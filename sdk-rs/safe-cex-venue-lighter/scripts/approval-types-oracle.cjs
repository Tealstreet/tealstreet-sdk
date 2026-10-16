const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.builder-approval.ts'),'utf8');
const ast=ts.createSourceFile('approval.ts',source,ts.ScriptTarget.Latest,true);
const names=['LighterBuilderApprovalContext','LighterBuilderApprovalCheckResult','LighterBuilderApprovalResult'];
const aliases=Object.fromEntries(names.map(name=>{const alias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);assert.ok(alias&&ts.isTypeLiteralNode(alias.type),name);return [name,alias.type];}));
const schemas=Object.fromEntries(names.map(n=>[n,aliases[n].members.map(m=>({name:m.name.text,type:m.type.getText(ast),optional:!!m.questionToken}))]));
const statusType=aliases.LighterBuilderApprovalCheckResult.members.find(m=>m.name.text==='approvalStatus').type;
assert.ok(ts.isUnionTypeNode(statusType));
const statuses=statusType.types.map(t=>{assert.ok(ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal));return t.literal.text;});
const fields=[];
for(const number of [-0,0,1.25,-2.5,16777217,1e100,NaN,Infinity,-Infinity])for(const present of [false,true])for(const bigint of ['123456789012345678901234567890','-123456789012345678901234567890'])fields.push({number,present,bigint});
const cardinalityCorpora={fields};
if(process.env.LIGHTER_TYPE_CARDINALITY_MUTATION) {
 const [field,mode]=process.env.LIGHTER_TYPE_CARDINALITY_MUTATION.split(':');
 cardinalityCorpora[field].splice(mode==='empty'?0:1);
}
process.stdout.write(JSON.stringify({schemas,statuses,fields},(_key,v)=>typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{$number:Object.is(v,-0)?'-0':String(v)}:v));
