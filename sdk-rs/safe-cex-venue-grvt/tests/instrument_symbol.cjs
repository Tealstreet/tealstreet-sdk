const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler&&path.isAbsolute(compiler),'absolute isolated compiler required');
const ts=require(compiler);
process.stderr.write(JSON.stringify({captureArchitecture:process.arch,node:process.version})+'\n');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=tree.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='GrvtExchange');
const method=cls.members.find(s=>s.name?.getText(tree)==='symbolToInstrumentId');
assert(method?.body,'actual symbol method');
let body=method.body.getText(tree);
if(process.env.GRVT_SYMBOL_CONTROL==='unanchored') {
 assert(body.includes('/_PERP$/i'));body=body.replace('/_PERP$/i','/_PERP/i');
}
const context={exports:{}};
vm.runInNewContext(ts.transpileModule('export function measured(symbol: string) '+body,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,context);
const rows=[];
for(const prefix of ['','BTC_USDT','eth_usdt','Mixed_PeRp','😀','é','\u0000','A\nB','＿PERP','_'])
 for(let mask=0;mask<16;mask++)for(const tail of ['','x','\n','\r','\r\n','\u2028','\u2029']) {
  const suffix=[...'PERP'].map((c,i)=>mask&(1<<i)?c.toLowerCase():c).join('');
  const input=prefix+'_'+suffix+tail;rows.push({input,expected:context.exports.measured(input)});
 }
for(const input of ['', 'PERP','_PER','_PERPP','BTC_PERP_USDT','BTC_KERP','BTC_PЕRP','BTC＿PERP'])rows.push({input,expected:context.exports.measured(input)});
const refusals=['"\\ud800_PERP"','"BTC_PERP\\udfff"'].map(raw=>({raw,expected:JSON.stringify(context.exports.measured(JSON.parse(raw)))}));
process.stdout.write(JSON.stringify({rows,refusals}));
