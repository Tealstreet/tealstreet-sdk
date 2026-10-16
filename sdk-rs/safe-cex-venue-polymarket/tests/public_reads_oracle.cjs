const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket');
const api = fs.readFileSync(path.join(root,'polymarket.api.ts'),'utf8');
const tree = ts.createSourceFile('api.ts',api,ts.ScriptTarget.Latest,true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'PolymarketAPI');
const method = cls.members.filter(n => n.name?.getText(tree) === 'getServerTime');
assert.equal(method.length,1,'actual_time_method');
const constants = fs.readFileSync(path.join(root,'polymarket.constants.ts'),'utf8');
const ct = ts.createSourceFile('constants.ts',constants,ts.ScriptTarget.Latest,true);
const decl = ct.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ct) === 'ENDPOINTS'));
assert.equal(decl.length,1,'actual_endpoint_declaration');
const original = constants.slice(decl[0].getFullStart(),decl[0].end) + '\nclass PolymarketAPI {' + api.slice(method[0].getFullStart(),method[0].end) + '}\nexports.API=PolymarketAPI;';
const context = {exports:{}};
vm.runInNewContext(ts.transpileModule(original,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, context);
const raw = ['1700000000','-0','0','1.25','-1.25','9007199254740993','1e400','-1e400','5e-324','1e-400','-1e-400','1e21','1.7976931348623157e308','2.2250738585072014e-308','"1700000000"','null','true','[]','{}','"-0"'];
(async () => {
 const cases=[];
 for (const input of raw) {
  const data=JSON.parse(input), calls=[];
  const api=new context.exports.API();
  api.xhr={get:async (...args)=>{calls.push(args);return {data};}};
  const output=await api.getServerTime();
  assert.strictEqual(output,data,'source_time_identity');
  const row={raw:input,calls,type:typeof output};
  if(typeof output==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(output);row.bits=b.toString('hex');}
  cases.push(row);
 }
 const failures=[];
 for(const code of ['ERR_NETWORK','ERR_BAD_RESPONSE','ERR_CANCELED']) {
  const error={message:'fixture failure',code,request:{url:'/changed'},response:{status:429,data:{error:'limit'}},context:{id:7}},calls=[];
  const api=new context.exports.API();api.xhr={get:async (...args)=>{calls.push(args);throw error;}};
  try {await api.getServerTime();assert.fail('source rejection required');} catch(actual){assert.strictEqual(actual,error,'source_failure_identity');failures.push({code,calls,preserved:true});}
 }
 process.stdout.write(JSON.stringify({capture:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0])},cases,failures}));
})().catch(error=>{console.error(error);process.exitCode=1;});
