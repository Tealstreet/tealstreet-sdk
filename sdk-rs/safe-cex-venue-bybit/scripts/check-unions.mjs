import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import ts from 'typescript';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const declarations=new Map();
for(const file of ['types','rateLimits','order-params','site']) {
 const source=ts.createSourceFile(file,readFileSync(`${root}packages/safe-cex/src/exchanges/bybit/bybit.${file}.ts`,'utf8'),ts.ScriptTarget.Latest,true);
 for(const node of source.statements) {
  if(ts.isTypeAliasDeclaration(node)||ts.isInterfaceDeclaration(node)||ts.isEnumDeclaration(node)) declarations.set(node.name.text,node);
  if(ts.isVariableStatement(node)) for(const d of node.declarationList.declarations) declarations.set(d.name.getText(source),d.initializer);
 }
}
function literals(node) {
 if(ts.isTypeAliasDeclaration(node)) return literals(node.type);
 if(ts.isEnumDeclaration(node)) return node.members.flatMap(m=>literals(m.initializer));
 if(ts.isUnionTypeNode(node)) return node.types.flatMap(literals);
 if(ts.isLiteralTypeNode(node)) return literals(node.literal);
 if(ts.isStringLiteral(node)) return [node.text];
 if(ts.isNumericLiteral(node)) return [Number(node.text)];
 if(ts.isTypeReferenceNode(node)) return literals(declarations.get(node.typeName.getText()));
 if(ts.isTypeQueryNode(node)) return literals(declarations.get(node.exprName.getText()));
 if(ts.isAsExpression(node)) return literals(node.expression);
 throw Error(`not a closed literal union: ${node.getText()}`);
}
function ownInterfaceMembers(node) {
 assert.ok(ts.isInterfaceDeclaration(node), 'expected an explicit interface field owner');
 // A field-selection oracle must not silently ignore inherited declarations.
 assert.ok(!node.heritageClauses?.length, 'Bybit union field census refuses interface heritage');
 return node.members;
}
const inherited=ts.createSourceFile('inherited-control.ts','interface ExtraNumeric { hiddenNumeric: number } interface Probe extends ExtraNumeric { category: "spot" }',ts.ScriptTarget.Latest,true);
assert.throws(()=>ownInterfaceMembers(inherited.statements[1]), /Bybit union field census refuses interface heritage/, 'inherited field must not disappear from the union census');
const expected={};
for(const name of ['BybitTransactionType','BybitVipLevel','BybitProductType','BybitAccountType','BybitBboSideType','BybitSiteId','BybitBboLevel']) expected[name]=[...new Set(literals(declarations.get(name)))];
for(const [name,field] of [['BybitTransactionAccountType','accountType'],['BybitTransactionCategory','category']]) expected[name]=literals(ownInterfaceMembers(declarations.get('BybitTransactionHistoryOptions')).find(m=>m.name.getText()===field).type);
const cases={};
for(const [name,values] of Object.entries(expected)) {
 cases[name]=[...new Set([...values,'','unknown','constructor','toString','__proto__',...values.flatMap(v=>[String(v).toLowerCase(),String(v).toUpperCase(),` ${v}`,`${v}\uFEFF`,`${v}\u0085`])]),null,true,false,0,6,1.5,[],{}];
}
// A separate probe records JSON representation widening rather than conflating it
// with enum spelling. These are deliberately not counted as matching literals.
const tagged={};
for(const [name,values] of Object.entries(expected)) if(name!=='BybitBboLevel') {
 tagged[name]=cases[name].length; cases[name].push({[values[0]]:null});
}
const params=declarations.get('BybitOrderExecutionParams');
const mode=literals(params.type.members.find(m=>m.name.getText()==='slippageToleranceType').type);
const levels=expected.BybitBboLevel.map(String);
const paramCases=[{value:{},accepted:true},...[[],['Queue','1','Percent','2'],null,true,1,''].map(value=>({value,accepted:false}))];
for(const [field,values] of [['bboLevel',levels],['slippageToleranceType',mode],['bboSideType',expected.BybitBboSideType],['slippageTolerance',['','1.5']]]) {
 for(const value of [...values,'not-a-literal',null,1,{},[]]) paramCases.push({value:{[field]:value},accepted:typeof value==='string'&&(field==='slippageTolerance'||values.includes(value))});
}
cases.BybitOrderExecutionParams=paramCases.map(c=>c.value);
const output={members:expected,cases:{}};
for(const [name,values] of Object.entries(expected)) {
 output.cases[name]=cases[name].map(value=>({value,accepted:values.includes(value)}));
 if(name!=='BybitBboLevel') {
  // Local and shared string unions reject externally tagged object forms,
  // including every variant.
  output.cases[name].pop();
  for(const value of values) output.cases[name].push({value:{[value]:null},accepted:false});
 }
}
output.cases.BybitOrderExecutionParams=paramCases;
// Ask the compiler about this actual declaration, including Record<string,any>:
// arrays are assignable there, despite primitive scalars and null being rejected.
const responsePath=`${root}packages/safe-cex/src/exchanges/bybit/bybit.response.ts`;
const responseSource=readFileSync(responsePath,'utf8');
const responseCases=[{isSuccess:true,hasExplicitStatus:false}];
for(const field of ['retCode','retMsg','rawResponse']) for(const value of [null,0,1.5,'','ok',false,{},[],[1,null]]) responseCases.push({...responseCases[0],[field]:value});
const augmented=responseSource+'\n'+responseCases.map((value,i)=>`const __acceptance_${i}: BybitResponseMeta = ${JSON.stringify(value)};`).join('\n');
const host=ts.createCompilerHost({strict:true});
const originalGet=host.getSourceFile.bind(host);
host.getSourceFile=(file,languageVersion,...rest)=>file===responsePath?ts.createSourceFile(file,augmented,languageVersion,true):originalGet(file,languageVersion,...rest);
const program=ts.createProgram([responsePath],{strict:true,noEmit:true},host);
const checker=program.getTypeChecker();
const file=program.getSourceFile(responsePath);
const probes=file.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).filter(d=>d.name.getText().startsWith('__acceptance_'));
assert.equal(probes.length,responseCases.length);
output.cases.BybitResponseMeta=probes.map((d,i)=>({value:responseCases[i],accepted:checker.isTypeAssignableTo(checker.getTypeAtLocation(d.initializer),checker.getTypeFromTypeNode(d.type))}));
console.log(JSON.stringify(output));
