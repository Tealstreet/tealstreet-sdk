const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/common.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_GENERIC_MUTATION;
if (mutation === 'inherited-number') source = source.replace(/(interface ApiResponse(?:<[^>]+>)?)(\s*\{)/, '$1 extends ExtraNumeric$2') + '\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
if (mutation === 'data-required') source = source.replace('data?: T;', 'data: T;');
if (mutation === 'items-scalar') source = source.replace('items: T[];', 'items: T;');
if (mutation === 'timestamp-string') source = source.replace('timestamp: number;', 'timestamp: string;');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const schemas = {};
for (const name of ['ApiResponse', 'PaginatedResponse']) {
  const node = tree.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.ok(node, name);
  assert.ok(!node.heritageClauses?.length, `${name} unsupported interface heritage`);
  schemas[name] = {
    parameters: node.typeParameters.map(t => t.getText(tree)),
    fields: node.members.map(f => ({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken})),
  };
}
const bits = value => { const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex'); };
const numeric = [mutation === 'erase-negative-zero' ? '0' : '-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25'].map(raw=>({raw,bits:bits(JSON.parse(raw))}));
const strings = ['plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text => {
  const encoded=JSON.stringify({code:0,message:text,data:1,error:text,timestamp:1});
  const parsed=JSON.parse(encoded);
  assert.equal(parsed.message,text);assert.equal(parsed.error,text);
  return {encoded,apiPayload:JSON.stringify({code:0,message:"fixture",timestamp:0,data:text}),pagePayload:JSON.stringify({items:[text],total:1,page:1,pageSize:1,totalPages:1,hasNext:false,hasPrev:false}),units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),representable:text.isWellFormed()};
});
const depthCases = [0,2,124,125,126,127,128,140].map(depth => {
  const payload='['.repeat(depth)+'17'+']'.repeat(depth);
  const api=`{"code":0,"message":"fixture","timestamp":0,"data":${payload}}`;
  const page=`{"items":[${payload}],"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false}`;
  for(const value of [JSON.parse(api).data,JSON.parse(page).items[0]]) {
    let cursor=value, measured=0;
    while(Array.isArray(cursor)) { assert.equal(cursor.length,1);cursor=cursor[0];measured++; }
    assert.equal(measured,depth);assert.equal(cursor,17);
  }
  return {depth,api,page};
});
const identity = {
  api: {code:0.25,message:'message-identity',data:{value:101.25},error:'error-identity',timestamp:202.5},
  page: {items:[{value:303.75},{value:404.5}],total:505.25,page:606.5,pageSize:707.75,totalPages:808.25,hasNext:true,hasPrev:false},
};
process.stdout.write(JSON.stringify({schemas,numeric,strings,depthCases,identity}));
