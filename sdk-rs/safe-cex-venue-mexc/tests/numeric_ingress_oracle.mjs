import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {root,ts} from './source.mjs';
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_NUMERIC_INGRESS_TS_CONTROL==='remove-number') {
 const before='code?: number | string;';
 assert(source.includes(before),'numeric-oracle mutation anchor');
 source=source.replace(before,'code?: string;');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const envelope=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='MexcEnvelope');
assert(envelope,'maintained MexcEnvelope declaration');
const code=envelope.type.members.find(n=>n.name?.getText(ast)==='code');
assert(code?.questionToken && ts.isUnionTypeNode(code.type),'optional code union');
assert.deepEqual(code.type.types.map(n=>n.kind).sort(),[ts.SyntaxKind.NumberKeyword,ts.SyntaxKind.StringKeyword].sort());
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const lexemes=['-0','-0.0','0','1','1.5','1e3','9007199254740993','1e400','-1e400'];
const cases=lexemes.map((lexeme,index)=>{
 const dataLexeme=lexemes[(index+2)%lexemes.length];
 const raw='{"code":'+lexeme+',"data":'+dataLexeme+'}';
 const parsed=JSON.parse(raw);
 return {lexeme,dataLexeme,raw,codeBits:bits(parsed.code),dataBits:bits(parsed.data)};
});
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
