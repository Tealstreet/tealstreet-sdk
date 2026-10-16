const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler),'absolute isolated compiler required');
const ts=require(compiler),{bits}=require('../../safe-cex-num/tests/support/number-bits.cjs');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=tree.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='GrvtExchange');
const member=cls.members.find(s=>s.name?.getText(tree)==='getInstrumentSigningMetadata');
assert(member?.initializer,'actual signing metadata method');
let text=member.initializer.getText(tree);
if(process.env.GRVT_METADATA_CONTROL==='nonnegative') {
 assert(text.includes('!Number.isSafeInteger(baseDecimals)'));
 text=text.replace('!Number.isSafeInteger(baseDecimals)','(!Number.isSafeInteger(baseDecimals) || baseDecimals < 0)');
}
const context={exports:{}};
vm.runInNewContext(ts.transpileModule('export const measured = '+text,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,context);
const tokens=['null','true','"3"','0','-0','-0.0','3','-3','1.25','1e3','9007199254740991','9007199254740992','-9007199254740991','1e400','-1e400','-1e-400','{}'];
const hashes=['0x0','0xAbCd','0x'+'f'.repeat(64),'0x'+'f'.repeat(65),'0x','0X1','0xz','0x1\n','abc','😀',''];
const rows=[];
function add(raw){
 try{const result=context.exports.measured(JSON.parse(raw));rows.push({raw,hash:result.ih,base:bits(result.bd),quote:bits(result.qd)});}
 catch(e){rows.push({raw,error:{name:e.name,message:e.message}});}
}
for(const hash of hashes)for(const base of tokens)for(const quote of tokens)
 add('{"id":"BTC mixed 😀","info":{"ih":'+JSON.stringify(hash)+',"bd":'+base+',"qd":'+quote+'}}');
for(const info of [undefined,'null','0','false','"value"','[]','{}']) add('{"id":"missing"'+(info===undefined?'':',"info":'+info)+'}');
for(const field of ['ih','bd','qd'])for(const value of [undefined,null,true,0,[],{}]) {
 const info={ih:'0xAb',bd:3,qd:7};
 if(value===undefined)delete info[field]; else info[field]=value;
 add(JSON.stringify({id:'field omission',info}));
}
const refusals=['{"id":"\\ud800","info":null}','{"id":"ok","info":{"ih":"\\udfff","bd":1,"qd":2}}'].map(raw=>{
 let observed;
 try{context.exports.measured(JSON.parse(raw));}catch(error){observed=error;}
 assert(observed && observed.name==='Error' && observed.message.endsWith('refresh markets before placing an order'),'surrogate source must refuse with metadata error');
 return {raw,name:observed.name,message:JSON.stringify(observed.message)};
});
process.stdout.write(JSON.stringify({rows,refusals}));
