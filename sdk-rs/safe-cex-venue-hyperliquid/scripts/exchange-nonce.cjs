const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const member=cls.members.find(n=>n.name?.getText(ast)==='generateNonce');assert(member);
let now=0;const out={};vm.runInNewContext(ts.transpileModule('class Subject { lastNonce=0; '+member.getText(ast)+' } out.Subject=Subject;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{out,Math,Date:{now:()=>now}});
const bits=require('./number-bits.cjs');
const sequences=[['0','0','-0','-1','2','1'],['9007199254740991','9007199254740992','9007199254740992'],['NaN','1'],['Infinity','1'],['-Infinity','0']];
const cases=[];for(const sequence of sequences){const s=new out.Subject();for(const input of sequence){now=Number(input);const result=s.generateNonce();cases.push({sequence:sequences.indexOf(sequence),input,bits:bits(result),lastBits:bits(s.lastNonce)});}}
const output=JSON.stringify({scope:'actual generateNonce body; lastNonce initial value 0',cases},null,2)+'\n';
const target=path.join(__dirname,'../tests/fixtures/exchange-nonce.json');if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'),'nonce source drift');
console.log(cases.length+' pre-serialization nonce bit cases');
