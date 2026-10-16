const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler_path_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const packageRequire = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../package.json'));
const createHmac = packageRequire('create-hmac');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.signing.ts');
const source = fs.readFileSync(file, 'utf8');
const compiled = ts.transpileModule(source, {fileName:file, reportDiagnostics:true, compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}});
assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0,'auth_transpile');
let clockValues, calls;
const clockFile = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/virtualClock.ts');
const clockSource = ts.createSourceFile(clockFile, fs.readFileSync(clockFile,'utf8'), ts.ScriptTarget.Latest, true);
const clockMembers=[];
function visit(node){if(ts.isPropertyDeclaration(node)&&node.name.getText(clockSource)==='getCurrentTimeMs')clockMembers.push(node);ts.forEachChild(node,visit);}
visit(clockSource);
assert.equal(clockMembers.length,1,'clock_member_census');
assert(ts.isArrowFunction(clockMembers[0].initializer),'clock_arrow_admission');
const clockArrow=ts.transpileModule('const read = '+clockMembers[0].initializer.getText(clockSource),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
let clockGet;
const context = vm.createContext({exports:{}, Date:{now(){assert(calls<clockValues.length,'clock_exhausted');return Number(clockValues[calls++]);}}, require:name=>{
 if(name==='create-hmac') return createHmac;
 if(name==='../../utils/virtualClock') return {virtualClock:{getCurrentTimeMs(){ return clockGet(); }}};
 throw Error(`unexpected_dependency:${name}`);
}});
clockGet=vm.runInContext('(function(){'+clockArrow+';return read;}).call({offsetMs:0})',context,{filename:clockFile});
vm.runInContext(compiled.outputText,context,{filename:file});
assert.deepEqual(Object.keys(context.exports).sort(),['generateAuthenticatedRequest','generateSignature','generateWsAuthMessage'].sort(),'auth_exports');
const inputs = [
 {method:'private/create-order',key:'key-A',secret:'secret-B',params:'{"z":"last","a":"first"}',id:'101.25',nonce:'202.5'},
 {method:'private/cancel-order',key:'key-C',secret:'secret-D',params:'{"2":2,"10":10,"🙂":"pair","\ue000":"bmp"}',id:'-0',nonce:'0'},
 {method:'',key:'',secret:'',params:'{}',id:'0',nonce:'-0'},
 {method:'méthod🙂',key:'k雪',secret:'s🔑',params:'{"a":["left",null,[1,2]],"b":-0}',id:'9007199254740993',nonce:'1e400'},
 {method:'private/read',key:'key-E',secret:'secret-F',params:'{"a":{"b":{"c":{"d":"deep"}}}}',id:'NaN',nonce:'-Infinity'},
 {method:'private/error',key:'key-G',secret:'secret-H',params:'{"a":{"b":{"toString":0}}}',id:'11',nonce:'22'},
 // Error-path supplement: reach String(object) at the source level-three cutoff.
 {method:'private/deep-error',key:'key-I',secret:'secret-J',params:'{"a":{"b":{"c":{"toString":0}}}}',id:'31',nonce:'62'},
];
const bits=n=>{if(Number.isNaN(n))return'7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const cases=[];
for(const input of inputs){
 for(const kind of ['signature','request','ws']){
  for(const explicit of [false,true]){
   clockValues=input.id==='NaN'?['NaN','-Infinity']:input.id==='9007199254740993'?['-0','1e400']:['303.75','404.125'];calls=0;
   const params=JSON.parse(input.params);
   let result;
   try{
    if(kind==='signature') result={sig:context.exports.generateSignature(input.method,Number(input.id),input.key,input.secret,params,Number(input.nonce))};
    if(kind==='request'){
     const value=context.exports.generateAuthenticatedRequest(input.method,params,input.key,input.secret,explicit?Number(input.nonce):undefined);
     result={id:bits(value.id),nonce:bits(value.nonce),method:value.method,key:value.api_key,sig:value.sig,paramsIdentity:value.params===params,fields:Object.keys(value)};
    }
    if(kind==='ws'){
     const value=context.exports.generateWsAuthMessage(input.key,input.secret);
     result={id:bits(value.id),nonce:bits(value.nonce),method:value.method,key:value.api_key,sig:value.sig,fields:Object.keys(value)};
    }
   }catch(e){result={error:{name:e.name,message:e.message}};}
   cases.push({input,kind,explicit,clockValues,calls,result});
  }
 }
}
assert.equal(cases.length,42,'auth_source_count');
console.log(JSON.stringify({header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),architecture:process.arch},cases}));
