const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BitgetExchange');
const member = klass.members.find(n => n.name?.getText(ast) === 'getAccount');
assert(member && ts.isPropertyDeclaration(member), 'ACCOUNT actual getAccount');
const protocolFile = path.join(root, 'packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
const protocol = ts.createSourceFile(protocolFile, fs.readFileSync(protocolFile, 'utf8'), ts.ScriptTarget.Latest, true);
const endpoints = protocol.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.text === 'ENDPOINTS'));
assert(endpoints, 'ACCOUNT actual endpoints');
const source = endpoints.getText(protocol).replace(/^export\s+/, '') + `\nclass Source {
  declare xhr: {get:(url:string)=>Promise<{data:any}>};
  ${member.getText(ast)}
}`;
const virtual = path.join(root, 'sdk-rs/safe-cex-venue-bitget/scripts/account-runtime.ts');
const options = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, strict: true, types: [], skipLibCheck: true };
const host = ts.createCompilerHost(options);
const original = host.getSourceFile.bind(host);
host.getSourceFile = (name, language, ...args) => name === virtual ? ts.createSourceFile(name, source, language, true) : original(name, language, ...args);
const program = ts.createProgram([virtual], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, `ACCOUNT source diagnostics: ${ts.formatDiagnosticsWithColorAndContext(diagnostics, {
  getCurrentDirectory: () => root, getCanonicalFileName: f => f, getNewLine: () => '\n',
})}`);
let emitted;
program.emit(undefined, (name, text) => { if (name.endsWith('account-runtime.js')) emitted = text; });
assert(emitted, 'ACCOUNT source emit');
const Source = new Function(emitted + ';return Source;')();

const names = ['missing', 'null', 'false', 'zero', 'negativeZero', 'empty', 'alpha', 'beta', 'number', 'array', 'object', 'poison', 'overflow'];
const values = [undefined, null, false, 0, -0, '', 'A-parent', 'B-child', 37, [null, 7], {}, {toString:null}, Infinity];
const encode = value => value === undefined ? {undefined:true} : typeof value === 'number' ? {number:Object.is(value,-0)?'-0':String(value)} : Array.isArray(value) ? value.map(encode) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key,item])=>[key,encode(item)])) : value;
(async () => {
  const cases = [];
  async function capture(input, body, fail) {
    const receiver = new Source(), calls = [];
    const failure = Object.assign(new Error('original read'), {name:'ReadFailure'});
    receiver.xhr = {get: url => {
      calls.push(url);
      if (fail === 'sync') throw failure;
      return fail === 'reject' ? Promise.reject(failure) : Promise.resolve({data:body});
    }};
    let output;
    try { output = {value:await receiver.getAccount()}; }
    catch(error) { output = {error:{name:error.name,message:error.message,same:error===failure}}; }
    cases.push({input,calls,output});
  }
  for (let p=0;p<13;p++) for (let u=0;u<13;u++) for (let i=0;i<13;i++) {
    const data = {};
    for (const [key,index] of [['parentId',p],['userId',u],['inviterId',i]]) if(index!==0) data[key]=values[index];
    await capture({kind:'fields',parent:names[p],user:names[u],inviter:names[i],values:[values[p],values[u],values[i]].map(encode)}, {data});
  }
  for (const placement of ['outer','inner']) for (const shape of ['missing','null','false','number','string','array','object']) {
    const value = {missing:undefined,null:null,false:false,number:7,string:'scalar',array:[],object:{}}[shape];
    await capture({kind:'shape',placement,shape}, placement==='outer'?value:{data:value});
  }
  for (const failure of ['sync','reject']) await capture({kind:'failure',failure}, {data:{}}, failure);
  const boundaries = [];
  for (const field of ['parentId','userId','inviterId']) {
    const receiver = new Source();
    receiver.xhr = {get:async()=>({data:{data:{[field]:'\ud800'}}})};
    const output = await receiver.getAccount();
    boundaries.push({field,output:JSON.stringify(output)});
  }
  await new Promise((resolve,reject)=>process.stdout.write(JSON.stringify({provenance:{architecture:process.arch,node:process.version},cases,boundaries})+'\n',error=>error?reject(error):resolve()));
})().catch(error=>{console.error(error);process.exitCode=1;});
