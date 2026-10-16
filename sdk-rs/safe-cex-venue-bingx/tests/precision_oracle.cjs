const assert=require('node:assert/strict'), fs=require('node:fs'), path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const classes=source.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text==='BingxExchange');
assert.equal(classes.length,1);
const methods=classes[0].members.filter(m=>ts.isMethodDeclaration(m)&&m.name?.getText(source)==='_parsePrecision');
assert.equal(methods.length,1);
const method=methods[0];
const compiled=ts.transpileModule(`(function(${method.parameters.map(p=>p.getText(source)).join(',')})${method.body.getText(source)})`,{
 reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});
assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));
const parse=vm.runInNewContext(compiled.outputText,{}, {filename:file});
const strings=['','0','-0','1','2','3','12','32','256','-3','2.9','0x10','0X3','+0x4','-0x3','0b11','0o7','08','3e2','  4tail','\uFEFF5','\u00856','NaN','Infinity','-Infinity','abc','３','3\ud800','\ud8003'];
const numbers=[NaN,Infinity,-Infinity,0,-0,-3,0.1,1,2,2.9,32,256,1e-7,1e21,1e22,1.2e25,5e-324,-1e308];
function bits(value){if(Number.isNaN(value))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex');}
const cases=[];
for(const text of strings.slice(0,-2))cases.push({kind:'string',text,expected:parse(text)});
for(const value of numbers)cases.push({kind:'number',bits:bits(value),expected:parse(value)});
const boundary=strings.slice(-2).map(text=>({units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),expected:parse(text)}));
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases,boundary}));
