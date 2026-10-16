// Offline capture at BOTH the venue->SDK and SDK->crypto/wire boundaries.
// Fixed key/clock/entropy are test-only. No network capability is supplied.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = p => fs.readFileSync(path.join(root, p), 'utf8');
const fixedNow = 1700000000000;
const secret = '01' + '00'.repeat(39);
class Clock extends Date { static now() { return fixedNow; } }
function load(file, imports = {}, extra = '') {
  const exports = {};
  let source = read(file);
  if (process.env.LIGHTER_SIGNING_IDENTITY_MUTATION === 'swap-fees' && file === 'packages/lighter-sdk/src/client/transaction-client.ts') {
    const maker = "this.toUint32FieldHex(params.maxPerpsMakerFee, 'maxPerpsMakerFee')";
    const taker = "this.toUint32FieldHex(params.maxSpotTakerFee, 'maxSpotTakerFee')";
    assert.equal(source.split(maker).length,2,'maker swap anchor');
    assert.equal(source.split(taker).length,2,'taker swap anchor');
    source = source.replace(maker,"this.toUint32FieldHex(params.maxSpotTakerFee, 'maxPerpsMakerFee')")
      .replace(taker,"this.toUint32FieldHex(params.maxPerpsMakerFee, 'maxSpotTakerFee')");
  }
  vm.runInNewContext(ts.transpileModule(source + extra, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText,
    { exports, Date: Clock, Buffer, TextEncoder, URLSearchParams, setTimeout, clearTimeout,
      require(name) { assert.ok(Object.hasOwn(imports, name), `unexpected import ${name}`); return imports[name]; } }, { filename: file });
  return exports;
}
function encode(value) {
  if (value === undefined) return { $undefined: true };
  if (typeof value === 'bigint') return { $bigint: String(value) };
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,encode(v)]));
  return value;
}
async function capture() {
  const context = vm.createContext({
    TextEncoder, TextDecoder, WebAssembly, Buffer, performance, setTimeout, clearTimeout,
    console: { log() {}, error: (...v) => console.error(...v) },
    crypto: { getRandomValues(bytes) { bytes.fill(0x42); return bytes; } },
  });
  vm.runInContext(read('packages/poseidon-crypto/src/wasm/wasm_exec.js'), context);
  const go = new context.Go();
  const wasm = fs.readFileSync(path.join(root, 'packages/poseidon-crypto/src/wasm/poseidon_crypto.wasm'));
  const instance = await WebAssembly.instantiate(wasm, go.importObject);
  go.run(instance.instance);
  assert.equal(typeof context.poseidon2HashFp5, 'function', 'real WASM fp5 primitive must be present');
  let current;
  const crypto = { isReady: () => true };
  for (const method of ['poseidon2Hash','poseidon2HashFp5','poseidon2HashTwoToOne','schnorrSign']) {
    crypto[method] = (...args) => {
      const result = context[method](...args);
      assert.equal(typeof result, 'string', `${method}: ${JSON.stringify(result)}`);
      current.crypto.push({ method, inputs: args.slice(0, method === 'schnorrSign' ? 1 : args.length), result });
      if (method === 'schnorrSign') {
        const pk = context.schnorrPkFromSk(secret);
        assert.equal(context.schnorrVerify(args[0], pk, result), true, 'captured signature verifies against real WASM');
      }
      return result;
    };
  }
  const { TransactionClient } = load('packages/lighter-sdk/src/client/transaction-client.ts');
  class CapturedTransactionClient extends TransactionClient {}
  for (const method of ['getCreateOrderTransaction','getCreateOrderTransactions','getCreateGroupedOrdersTransaction','getCancelOrderTransaction','getUpdateLeverageTransaction','getApproveIntegratorTransaction','getAuthToken']) {
    CapturedTransactionClient.prototype[method] = async function(...args) {
      current.sdkCalls.push({ method, args: encode(args) });
      const result = await TransactionClient.prototype[method].apply(this,args);
      current.sdkResults.push({ method, result: encode(result) });
      return result;
    };
  }
  const httpTypes = load('packages/lighter-sdk/src/client/http-types.ts');
  const axios = { create() { return {
    interceptors: { request: { use() {} }, response: { use() {} } },
    async post(url, body, config) {
      current.wire.push({url,body,config});
      if (url.endsWith('sendTxBatch')) return {data:{code:200,tx_hash:JSON.parse(new URLSearchParams(body).get('tx_infos')).map((_,i)=>`offline-${i}`)}};
      return {data:{code:200,tx_hash:'offline-only'}};
    },
  }; } };
  const { LighterApiClient } = load('packages/lighter-sdk/src/client/api-client.ts', { axios, './http-types': httpTypes });
  const domainAst = ts.createSourceFile('types.ts',read('packages/safe-cex/src/types.ts'),ts.ScriptTarget.Latest,true);
  const enums = {};
  vm.runInNewContext(ts.transpileModule(domainAst.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domainAst)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
  const profiles = load('packages/safe-cex/src/exchanges/lighter/lighter.types.ts', {'../../types':enums,'../../transport/registry':{withPublicCorsProxy(){throw Error('network forbidden')}}});
  const signer = load('packages/safe-cex/src/exchanges/lighter/lighter.signer.ts', {
    '@tealstreet/lighter-sdk': {LighterApiClient,TransactionClient:CapturedTransactionClient},
    '@tealstreet/poseidon-crypto': {getCryptoApi:()=>crypto,initWasm:async()=>{},isWasmReady:()=>true},
    './lighter.types':profiles,
  });
  const order = {orderId:123n,marketId:1,isBuy:true,price:100000n,amount:50000n,postOnly:false,reduceOnly:false,orderType:'limit'};
  const complete = {...order,triggerPrice:0n,timeInForce:'GoodTillCancel',integratorAccountIndex:158385n,integratorMakerFee:0,integratorTakerFee:250};
  const approval = {integratorAccountIndex:158385n,maxPerpsTakerFee:250,maxPerpsMakerFee:0,maxSpotTakerFee:0,maxSpotMakerFee:0,approvalExpiry:1702592000000n};
  let inputs = [
    ['create-missing-optionals','createOrder',[order]],
    ['create-complete','createOrder',[complete]],
    ['create-explicit-undefined','createOrder',[{...order,triggerPrice:undefined,timeInForce:undefined}]],
    ['create-partial-integrator','createOrder',[{...order,integratorAccountIndex:158385n,integratorTakerFee:250}]],
    ['create-market','createOrder',[{...order,orderType:'market'}]],
    ['create-post-only','createOrder',[{...order,postOnly:true}]],
    ['create-fill-or-kill','createOrder',[{...order,timeInForce:'FillOrKill'}]],
    ['create-take-profit','createOrder',[{...order,orderType:'take_profit',triggerPrice:110000n}]],
    ['create-missing-integrator-index','createOrder',[{...order,integratorMakerFee:0,integratorTakerFee:250}]],
    ['create-missing-integrator-taker','createOrder',[{...order,integratorAccountIndex:158385n,integratorMakerFee:0}]],
    ['create-zero-integrator','createOrder',[{...order,integratorAccountIndex:0n,integratorMakerFee:0,integratorTakerFee:0}]],
    ['create-empty-batch','createOrders',[[]]],
    ['create-conditional','createOrder',[{...order,orderType:'stop_loss',triggerPrice:99000n,timeInForce:'ImmediateOrCancel',reduceOnly:true}]],
    ['create-batch','createOrders',[[complete,{...order,orderId:124n,isBuy:false}]]],
    ['create-grouped-oco','createGroupedOrders',[{groupingType:2,orders:[{...order,orderType:'stop_loss',triggerPrice:90000n,reduceOnly:true},{...order,orderId:124n,orderType:'take_profit',triggerPrice:110000n,reduceOnly:true}],integratorAccountIndex:158385n,integratorMakerFee:0,integratorTakerFee:250}]],
    ['create-grouped-oto','createGroupedOrders',[{groupingType:1,orders:[order,{...order,orderId:124n,isBuy:false,amount:0n,orderType:'stop_loss',triggerPrice:90000n}]}]],
    ['create-grouped-otoco','createGroupedOrders',[{groupingType:3,orders:[order,{...order,orderId:124n,isBuy:false,amount:0n,orderType:'stop_loss',triggerPrice:90000n},{...order,orderId:125n,isBuy:false,amount:0n,orderType:'take_profit',triggerPrice:110000n}],integratorAccountIndex:158385n,integratorMakerFee:0,integratorTakerFee:250}]],
    ['cancel-large-index','cancelOrder',[{orderId:9007199254740993n,marketId:1}]],
    ['leverage','updateLeverage',[{marketId:1,leverage:3,marginMode:1}]],
    ['approve-missing-l1','approveIntegrator',[approval]],
    ['approve-present-l1','approveIntegrator',[{...approval,l1Signature:'0x1234'}]],
    ['approve-revoke','approveIntegrator',[{...approval,maxPerpsTakerFee:0,approvalExpiry:0n}]],
    ['auth','getAuthToken',[3600]],
  ];
  const identitySupplement = process.env.LIGHTER_SIGNING_IDENTITY_SUPPLEMENT === '1';
  if (identitySupplement) {
    const distinct = {...approval,maxPerpsTakerFee:251,maxPerpsMakerFee:17,maxSpotTakerFee:43,maxSpotMakerFee:89};
    inputs = [
      ['approve-distinct-fees','approveIntegrator',[distinct]],
      ['approve-distinct-fees-l1','approveIntegrator',[{...distinct,l1Signature:'0x1234'}]],
      ...['maxPerpsTakerFee','maxPerpsMakerFee','maxSpotTakerFee','maxSpotMakerFee'].map((field,i)=>[
        `approve-field-${field}`,'approveIntegrator',[{...distinct,[field]:101+i}]
      ]),
    ];
  }
  const cases = [];
  for (const exchangeName of ['lighter','lighter-robinhood']) for (const testnet of [false,true]) for (const [name,method,args] of inputs) {
    current = {name:`${exchangeName}/${testnet?'testnet':'livenet'}/${name}`,outer:{method,args:encode(args),adapterConfig:encode({exchangeName,testnet,accountIndex:42n,apiKeyIndex:3})},sdkCalls:[],sdkResults:[],crypto:[],wire:[]};
    const api = new LighterApiClient({baseUrl:'https://offline.invalid'});
    api.getNextNonce = async (...args) => { current.nonceRequest=encode(args);return 7n; };
    const adapter = await signer.createLighterSignerAdapter({exchangeName,options:{secret,key:'',testnet},apiClient:api,accountIndex:42n,apiKeyIndex:3});
    current.result=encode(await adapter[method](...args));
    cases.push(current);
  }
  const files = ['packages/lighter-sdk/package.json','packages/poseidon-crypto/package.json',
    'packages/safe-cex/src/types.ts','packages/safe-cex/src/exchanges/lighter/lighter.signer.ts','packages/safe-cex/src/exchanges/lighter/lighter.types.ts',
    'packages/lighter-sdk/src/client/transaction-client.ts','packages/lighter-sdk/src/client/api-client.ts','packages/lighter-sdk/src/client/http-types.ts',
    'packages/poseidon-crypto/src/wasm/wasm_exec.js','packages/poseidon-crypto/src/wasm/main.go','packages/poseidon-crypto/src/wasm/poseidon_crypto.wasm'];
  const sources = Object.fromEntries(files.map(file => [file,createHash('sha256').update(fs.readFileSync(path.join(root,file))).digest('hex')]));
  const versions = Object.fromEntries(['lighter-sdk','poseidon-crypto'].map(name => [name,JSON.parse(read(`packages/${name}/package.json`)).version]));
  return {captureHeader:{...(identitySupplement?{consumerGate:'venue-oracle:lighter / signing_field_identity_supplement_matches_shared_crypto; shared safe-cex-crypto independent_lighter_field_identity adopted at 74aa2775f1 (published, integration and live CI unclaimed)',captureArchitecture:process.arch,capturePlatform:process.platform,nodeVersion:process.version,originalArtifactSha256:'afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a'}:{}),purpose:identitySupplement ? 'Later field-identity supplement; no retroactive pre-implementation independence claim. Original 92-case artifact remains byte-identical. Intended companion to safe-cex-crypto/tests/independent_lighter.rs; runtime wiring status recorded in GATE_EVIDENCE.md. Original approval maxPerpsMakerFee/maxSpotTakerFee/maxSpotMakerFee were all zero, so their swaps were invisible; this is corpus exposure, not a signer defect claim.' : 'Independent offline TypeScript capture before Rust signing integration',versionPin:'Both package versions are 0.0.1; source and WASM SHA256s below are the artifact pins',deploymentProfiles:profiles.LIGHTER_DEPLOYMENT_PROFILES,boundaries:['lighter.signer method arguments and adapter configuration','TransactionClient method arguments and results','real WASM hash/sign preimages and outputs','LighterApiClient serialized wire requests'],keyPurpose:'Fixed public test-only offline key; never a venue credential'},versions,sources,fixedNow,secret,publicKey:context.schnorrPkFromSk(secret),entropyByte:66,cases};
}
capture().then(result => {
  const text = JSON.stringify(result,null,2)+'\n';
  if (process.argv[2]) { fs.writeFileSync(process.argv[2],text); process.exit(0); }
  else process.stdout.write(text, () => process.exit(0));
}).catch(error => {console.error(error);process.exit(1);});
