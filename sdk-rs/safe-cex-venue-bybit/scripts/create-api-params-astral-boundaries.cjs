const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require('axios');
const retry = require('axios-retry');
const qs = require('qs');

const root = path.resolve(process.env.SAFE_CEX_ROOT || path.join(__dirname, '../../..'));
function parse(file, text = fs.readFileSync(path.join(root, file), 'utf8')) {
  const parsed = ts.createSourceFile(file, text, 99, true);
  assert.equal(parsed.parseDiagnostics.length, 0, `source AST diagnostics: ${file}`);
  return parsed;
}
function load(file, imports = {}, text, outputConsole = console) {
  const source = parse(file, text);
  const compiled = ts.transpileModule(source.text, {
    compilerOptions: { target: 7, module: 1, esModuleInterop: true },
    reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.length, 0, `transpile diagnostics: ${file}`);
  const emitted = ts.createSourceFile(file + '.js', compiled.outputText, 99, true, ts.ScriptKind.JS);
  assert.equal(emitted.parseDiagnostics.length, 0, `emitted AST diagnostics: ${file}`);
  const context = vm.createContext({
    exports: {},
    console: outputConsole,
    require(name) {
      assert.ok(Object.hasOwn(imports, name), `undeclared import: ${name}`);
      return imports[name];
    },
  });
  vm.runInContext(compiled.outputText, context);
  return context.exports;
}

const apiPath = 'packages/safe-cex/src/exchanges/bybit/bybit.api.ts';
const apiSource = parse(apiPath);
const moduleFunctions = [];
let factory;
for (const statement of apiSource.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (!declaration.initializer || !ts.isArrowFunction(declaration.initializer)) continue;
    const name = declaration.name.getText(apiSource);
    moduleFunctions.push(name);
    if (name === 'createAPI') factory = declaration.initializer;
  }
}
assert.deepEqual(moduleFunctions, [
  'isSignedRequest', 'isPublicEndpoint', 'isInstrumentsInfoRequest', 'isBybitClockSyncRetMsg',
  'parseBybitServerTimestamp', 'hasRetriedClockSync', 'isBybitEnvProbeRequest',
  'shouldRetryBybitRequest', 'createAPI',
], 'independent module function inventory');
assert.ok(factory);
const callbacks = [];
function visit(node) {
  if (node !== factory && (ts.isArrowFunction(node) || ts.isFunctionExpression(node))) {
    const parent = node.parent;
    if (ts.isVariableDeclaration(parent)) callbacks.push(parent.name.getText(apiSource));
    else if (ts.isPropertyAssignment(parent)) callbacks.push('paramsSerializer.' + parent.name.getText(apiSource));
    else if (ts.isCallExpression(parent)) callbacks.push(parent.expression.getText(apiSource));
    else assert.fail('unclassified createAPI callback');
  }
  ts.forEachChild(node, visit);
}
visit(factory);
assert.deepEqual(callbacks, [
  'paramsSerializer.serialize', 'getOppositeBybitEnvironmentProbe', 'probeOppositeBybitEnvironment',
  'getBybitEnvProbeResult', 'maybePrependBybitEnvMismatch',
  'xhr.interceptors.request.use', 'xhr.interceptors.response.use',
], 'independent createAPI callback inventory');

const typesPath = 'packages/safe-cex/src/exchanges/bybit/bybit.types.ts';
const typesSource = parse(typesPath);
const typeNames = ['RECV_WINDOW', 'BROKER_ID', 'BASE_URL', 'DEMO_URL', 'ENDPOINTS'];
const declarations = typesSource.statements.filter(statement =>
  ts.isVariableStatement(statement) && statement.declarationList.declarations.some(declaration =>
    typeNames.includes(declaration.name.getText(typesSource))));
assert.equal(declarations.length, 5);
const typesProgram = declarations.map(declaration => declaration.getText(typesSource)).join('\n');
parse('assembled-bybit-api-constants.ts', typesProgram);
const types = load('assembled-bybit-api-constants.ts', {}, typesProgram);
assert.deepEqual(Object.keys(types).sort(), [...typeNames].sort());
const response = load('packages/safe-cex/src/exchanges/bybit/bybit.response.ts');
const retryReads = load('packages/safe-cex/src/utils/retryReadsOnly.ts', { 'axios-retry': retry });

const payloadSigner = load('packages/safe-cex/src/utils/payloadSigner.ts', { 'create-hmac': require('create-hmac') });
const site = load('packages/safe-cex/src/exchanges/bybit/bybit.site.ts');

const units = value => Array.from({length:value.length}, (_, index) => value.charCodeAt(index));
async function observeAstral(signed) {
  const options = {key:signed?'key':'',secret:signed?'secret':''};
  let client, captured, physical=0, nextId=0, authSuccesses=0, clockCalls=0;
  const payloads=[];
  const api=load(apiPath, {
    axios, 'axios-retry':retry, qs, nanoid:{nanoid:()=>`id-${++nextId}`},
    '../../remoteSigningAdapter':{markSkipRemoteSigning(){assert.fail('private input');}},
    '../../safeCexAxiosCreate':{
      safeCexAxiosCreate(config,caller,exchange){
        assert.equal(caller,options); assert.equal(exchange.name,'bybitinverse');
        client=axios.create({...config,timeout:20000,adapter:async current=>{
          physical++;captured=current;
          return {status:200,statusText:'OK',headers:{},config:current,data:{retCode:0,retMsg:'OK'}};
        }});return client;
      },
      installTransportAdapter(){assert.fail('no tunnel');},
    },
    '../../utils/payloadSigner':{
      canSignPayload:payloadSigner.canSignPayload,
      signPayload(options,exchange,payload,encoding){payloads.push(payload);return payloadSigner.signPayload(options,exchange,payload,encoding);},
    },
    '../../utils/runtime':{isNodeOrBunRuntime:()=>true},
    '../../utils/virtualClock':{virtualClock:{getCurrentTimeMs(){clockCalls++;return 1000;}}},
    './bybit.response':response,'./bybit.site':site,'./bybit.types':types,'../../utils/retryReadsOnly':retryReads,
  },apiSource.text,{error(){assert.fail('unexpected diagnostic');}});
  const xhr=api.createAPI(options,{name:'bybitinverse',isDemo:false,
    recordAuthSuccess(){authSuccesses++;},recordAuthFailure(){assert.fail('unexpected auth failure');},emitAndLogError(){assert.fail('unexpected error');}});
  let result,rejection;
  try{result=await xhr.request({method:'get',url:'/private',params:'😀'});}catch(error){rejection=error;}
  assert.equal(rejection,undefined,'actual scalar-spread request resolves');
  assert.equal(physical,1,'fixed source physical floor');assert.equal(nextId,1,'fixed source request-ID floor');
  assert.equal(authSuccesses,Number(signed));assert.equal(clockCalls,Number(signed));
  assert.equal(result.config,captured,'original config identity');
  const ownParams=Object.keys(captured.params).map(key=>({key,units:units(captured.params[key])}));
  assert.deepEqual(units('😀'),[55357,56832],'well-formed scalar input code units');
  assert.deepEqual(ownParams,[
    {key:'0',units:[55357]},{key:'1',units:[56832]},{key:'requestId',units:[105,100,45,49]},
  ],'source object spread materializes separate UTF16 code units');
  const wireQuery=client.getUri(captured).split('?')[1];
  const expectedQuery='0=%F0%9F%90%80&1=%F2%90%80%80&requestId=id-1';
  assert.equal(wireQuery,expectedQuery,'actual qs encodes the materialized source units');
  assert.deepEqual(payloads,signed?['1000key20000'+expectedQuery]:[],'actual signer sees the encoded units');
  assert.equal(captured.headers.get('X-BAPI-SIGN')??null,signed?'8bf83e68e838b2cbbee0dfe75a9df745e9e395b40bf78b6a1af854720b4515a0':null,'independently pinned actual source HMAC');
  return {id:signed?'signed-astral-string':'unsigned-astral-string',inputUnits:units('😀'),physical,nextId,authSuccesses,clockCalls,ownParams,wireQuery,payloads,signature:captured.headers.get('X-BAPI-SIGN')??null};
}
(async()=>{
  const cases=[];
  for(const signed of [false,true])cases.push(await observeAstral(signed));
  assert.equal(cases.length,2,'fixed astral-string source floor');
  process.stdout.write(JSON.stringify({scope:'source-only; unchanged whole maintained Bybit API plus actual Axios/retry/qs/payloadSigner/site; scalar input string; UTF16 observer uses code-unit indexing; no native run',declaredCases:2,cases})+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
