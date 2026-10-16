// Live capture architecture is recorded per case; these JSON lexemes cannot produce NaN.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const records=JSON.parse(execFileSync(process.execPath,[fileURLToPath(new URL('./check-shapes.mjs',import.meta.url)),'--records'],{encoding:'utf8'}));
const paths=(value,prefix=[])=> typeof value==='number'?[prefix]:value&&typeof value==='object'?Object.entries(value).flatMap(([key,value])=>paths(value,[...prefix,key])):[];
const bits=value=>{const b=new ArrayBuffer(8);const v=new DataView(b);v.setFloat64(0,value);return v.getBigUint64(0).toString(16).padStart(16,'0');};
// The placement decoder also buffers a map; cover each variant's numeric inputs.
records.BybitPlaceOrderOptsPlain={symbol:'BTCUSDT',type:'limit',side:'buy',amount:1,price:2,triggerPrice:3,stopLoss:4,takeProfit:5,maxSlippagePct:6,impliedVolatility:7};
records.BybitPlaceOrderOptsBbo={symbol:'BTCUSDT',type:'limit',side:'buy',amount:1,bybitBbo:{sideType:'Queue',level:1}};
records.BybitPlaceOrderOptsTickSize={symbol:'BTCUSDT',type:'market',side:'buy',amount:1,bybitSlippageToleranceTicks:10};
const source=ts.createSourceFile('types.ts',readFileSync(new URL('../../../packages/safe-cex/src/types.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const placement=source.statements.find(node=>ts.isTypeAliasDeclaration(node)&&node.name.text==='PlaceOrderOpts');
assert.ok(placement&&ts.isTypeLiteralNode(placement.type),'live PlaceOrderOpts required');
assert.deepEqual(placement.type.members.filter(member=>member.type?.kind===ts.SyntaxKind.NumberKeyword).map(member=>member.name.getText(source)),['amount','price','triggerPrice','stopLoss','takeProfit','maxSlippagePct','impliedVolatility']);
// Exercise the declared opaque record on both nested-object and array paths.
assert.ok(records.BybitResponseMeta, 'live response-meta record required');
records.BybitResponseMeta.rawResponse={nested:[1,{value:2}]};
const cases=[];
for(const [record,base] of Object.entries(records))for(const path of paths(base).filter(path=>path.join('/')!=='bybitBbo/level'))for(const lexeme of ['-0','-0.0','0','1e400','-1e400','1e-400','-1e-4000','1.25','1e3']){
 const input=structuredClone(base);let parent=input;for(const key of path.slice(0,-1))parent=parent[key];parent[path.at(-1)]='__numeric_lexeme__';
 const text=JSON.stringify(input).replace('"__numeric_lexeme__"',lexeme);
 let decoded=JSON.parse(text);for(const key of path)decoded=decoded[key];
 assert.ok(!Number.isNaN(decoded), "numeric ingress JSON lexemes cannot capture NaN");
 cases.push({captureArchitecture:process.arch,record,path:'/'+path.join('/'),lexeme,text,bits:bits(decoded)});
}
assert.equal(cases.length,306);
assert.equal(cases.filter(c=>c.lexeme==='-0').every(c=>c.bits==='8000000000000000'),true);
console.log(JSON.stringify(cases));
