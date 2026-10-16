const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.signer.ts'),'utf8');
const ast=ts.createSourceFile('signer.ts',source,ts.ScriptTarget.Latest,true);
const names=['LighterSignerProvider','LighterCreateOrderRequest','LighterCreateGroupedOrdersRequest','LighterCancelOrderRequest','LighterUpdateLeverageRequest','LighterApproveIntegratorRequest'];
const aliases=Object.fromEntries(names.map(name=>{const alias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);assert.ok(alias,name);return [name,alias.type];}));
const schema=type=>ts.isTypeLiteralNode(type)?type.members.map(m=>({name:m.name.text,type:m.type.getText(ast),optional:!!m.questionToken})):type.getText(ast);
const literalValues=type=>(ts.isUnionTypeNode(type)?type.types:[type]).map(t=>{assert.ok(ts.isLiteralTypeNode(t));const l=t.literal;assert.ok(ts.isStringLiteral(l)||ts.isNumericLiteral(l));return ts.isNumericLiteral(l)?Number(l.text):l.text;});
const field=(name,key)=>aliases[name].members.find(m=>m.name.text===key).type;
const unions={provider:literalValues(aliases.LighterSignerProvider),orderType:literalValues(field('LighterCreateOrderRequest','orderType')),timeInForce:literalValues(field('LighterCreateOrderRequest','timeInForce')),groupingType:literalValues(field('LighterCreateGroupedOrdersRequest','groupingType')),marginMode:literalValues(field('LighterUpdateLeverageRequest','marginMode'))};
const schemas=Object.fromEntries(names.map(n=>[n,schema(aliases[n])]));
const numeric=[-Infinity,-1,-0,0,0.5,1,1.5,2,3,4,NaN,Infinity,'0','1',null,false,true,[],{}, {'0':null},{'1':null}];
const accepted=Object.fromEntries(['groupingType','marginMode'].map(k=>[k,numeric.map(value=>({value,expected:typeof value==='number'&&unions[k].includes(value)}))]));
const fields=[];
for(let seed=0;seed<16;seed++)for(const present of [false,true]){
 const number=[-0,0,1.25,-2.5,16777217,1e100,NaN,Infinity,-Infinity][seed%9];
 const bigint=(seed%2?'-':'')+'123456789012345678901234567890'+seed;
 fields.push({seed,present,number,bigint});
}
const cardinalityCorpora={fields,groupingType:accepted.groupingType,marginMode:accepted.marginMode};
if(process.env.LIGHTER_TYPE_CARDINALITY_MUTATION) {
 const [field,mode]=process.env.LIGHTER_TYPE_CARDINALITY_MUTATION.split(':');
 cardinalityCorpora[field].splice(mode==='empty'?0:1);
}
process.stdout.write(JSON.stringify({schemas,unions,accepted,fields},(_key,v)=>typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{$number:Object.is(v,-0)?'-0':String(v)}:v));
