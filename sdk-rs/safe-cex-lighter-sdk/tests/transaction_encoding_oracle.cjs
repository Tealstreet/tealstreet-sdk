const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function extract(relative,names){const file=path.join(root,relative),src=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);return names.map(name=>{const hits=src.statements.filter(n=>n.name?.text===name);assert.equal(hits.length,1,name+' maintained source');return hits[0].getText(src);}).join('\n');}
const amountNames=['bigIntToHex','formatAmount','parseAmount'];
const declarations=extract('packages/lighter-sdk/src/utils/index.ts',amountNames);
const compiled=ts.transpileModule(declarations+'\n({'+amountNames.join(',')+'})',{fileName:'maintained-encoding.ts',reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));
const source=vm.runInNewContext(compiled.outputText,{exports:{}});
function units(s){return Array.from({length:s.length},(_,i)=>s.charCodeAt(i));}
function pack(v){if(typeof v==='string')return{units:units(v)};if(typeof v==='bigint')return{bigint:String(v)};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:Number.isNaN(v)?'NaN':b.toString('hex')}};if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,pack(v)]));return v;}
function run(method,args,label){let output;try{output={ok:pack(source[method](...args))};}catch(e){output={error:{name:e.name,message:units(e.message)}}}return{method,label,args:pack(args),output};}
const cases=[];
const utilityArgs={
 bigIntToHex:[[255n],[255n,4],[-255n,4],[0n,-0],[2n**300n,1],[255n,3.75],[12n,NaN],[255n,-2],[1n,Infinity],[1n,0.5],[1n,0],[1n,-Infinity]],
 formatAmount:[[123456789n,8],[100000000n,8],[-1230n,3],[-5n,3],[2n**300n,0],[1n,-0],[1n,-1],[1n,0.5],[1n,NaN],[1n,Infinity],[1n,-Infinity],[0n,2]],
 parseAmount:[['1.234',3],['0001.2',3],['12.',0],['12',-0],['999999999999999999999999999999999',2],['1.23',2.5],['1.2345',2],['-1',2],['1\ud800',2],['12.34',NaN],['1\n',0],['1',-1],['1',Infinity],['',2],['.1',1],['1.2.3',2]]
};
for(const method of amountNames)utilityArgs[method].forEach((args,i)=>cases.push(run(method,args,i)));
process.stdout.write(JSON.stringify({runtime:{node:process.version,platform:process.platform,arch:process.arch},cases}));
