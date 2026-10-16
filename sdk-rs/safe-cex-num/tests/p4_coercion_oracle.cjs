// Measured WOO Number guard, bitget fills/income and independent lighter prefix cases.
const fs=require('node:fs'),path=require('node:path');const {root,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const input=process.argv[2];const enc=n=>Number.isNaN(n)?'NaN':Object.is(n,-0)?'-0':String(n);
const measured=['',' ','\ufeff12.5tail','\u008512.5','12.5tail','0x10','0b10','0o10','Infinity','-Infinity','1e309','-0','12abc','1e+21','+12.5','0x','0b','0o'];
let seed=412313;const rand=()=>seed=(Math.imul(seed,1664525)+1013904223)>>>0;
function strings(){const rows=[...measured];const prefixes=['',' ','\ufeff','\u0085','\t','+','-','0x','0b','0o'];const suffixes=['','tail','e','e+','e-4','E+2','.3','Infinity','_','é'];for(let i=0;i<10000;i++)rows.push(prefixes[rand()%prefixes.length]+String(rand()%100000)+(rand()%3===0?'.'+String(rand()%100):'')+suffixes[rand()%suffixes.length]);return rows;}
let rows;
if(input==='number')rows=strings().map(text=>({text,expected:enc(Number(text))}));
else if(input==='float')rows=strings().map(text=>({text,expected:enc(parseFloat(text))}));
else if(input==='int') {rows=[];for(const text of strings())for(const radix of [0,10,16])rows.push({text,radix,expected:enc(parseInt(text,radix))});for(let radix=2;radix<=36;radix++)for(const text of ['0x10','-0','ff','123456789012345678901234567890','zzzzzzzzzzzzzzzzzz','1010101010101010101010101010101010101010101010101010101010101010101'])rows.push({text,radix,expected:enc(parseInt(text,radix))});}
else if(input==='string') {const raws=['null','true','false','0','-0','1e21','1e309','""','"12.5tail"','[]','[null]','[12.5]','[[],[null,12.5],{}]','{}','{"toString":null}','{"toString":1}','[{"toString":false}]'];rows=raws.map(raw=>{let expected;try{expected={ok:String(JSON.parse(raw))}}catch(e){expected={error:{name:e.name,message:e.message}}}return {raw,expected};});rows.push({raw:null,expected:{ok:'undefined'}});}
else if(input==='measured') {
 const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.income.ts'),'utf8');const tree=ts.createSourceFile('income.ts',source,ts.ScriptTarget.Latest,true);const stmt=tree.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(tree)==='num'));if(!stmt)throw Error('bitget num call site missing');const code=ts.transpileModule(stmt.getText(tree)+'\nexport {num};',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;const exports={};new Function('exports',code)(exports);
 const values=[null,true,'',[],[null],{},[12.5],...measured];rows=values.map(value=>({value,expected:enc(exports.num(value))}));
}
else throw Error('unknown corpus');
process.stdout.write(JSON.stringify(rows));
