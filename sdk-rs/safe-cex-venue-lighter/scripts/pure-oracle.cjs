const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = p => {
  let source = fs.readFileSync(path.join(root, p), 'utf8');
  if (process.env.LIGHTER_ORACLE_MUTATION === 'bigint-number' && p.endsWith('/lighter.utils.ts')) {
    const original = 'return BigInt(value);';
    assert.equal(source.split(original).length, 2, 'stringToBigInt mutation anchor');
    source = source.replace(original, 'return BigInt(Number(value));');
  }
  if (p.endsWith('/lighter.builder-approval.ts')) {
    const replacements = {
      'approval-bigint-direct': ['return BigInt(String(value));', 'return BigInt(value);'],
      'approval-message-label': ['account index: ${toHex10(params.accountIndex', 'accountIndex: ${toHex10(params.accountIndex'],
      'approval-hex-width': ["padStart(16, '0')", "padStart(10, '0')"],
    };
    const pair = replacements[process.env.LIGHTER_ORACLE_MUTATION];
    if (pair) {
      assert.equal(source.split(pair[0]).length, 2, 'approval mutation anchor');
      source = source.replace(...pair);
    }
  }
  if (process.env.LIGHTER_ORACLE_MUTATION === 'scale-round-trunc' && p.endsWith('/lighter.utils.ts')) {
    const original = 'BigInt(Math.round(value * scale))';
    assert.equal(source.split(original).length, 2, 'toLighterScaledInt mutation anchor');
    source = source.replace(original, 'BigInt(Math.trunc(value * scale))');
  }
  if (process.env.LIGHTER_ORACLE_MUTATION === 'funding-floor' && p.endsWith('/lighter.ws-public.ts')) {
    const original = 'fundingRatePercentPerHour === 0.0012';
    assert.equal(source.split(original).length, 2, 'normalizeLighterFundingRate mutation anchor');
    source = source.replace(original, 'fundingRatePercentPerHour === 0.0013');
  }
  if (p.endsWith('/lighter.ws-policy.ts')) {
    const pair = {
      'backoff-min': ['Math.min(failureCount, maxExponent)', 'Math.max(failureCount, maxExponent)'],
      'public-rate-max': ['return params.isRateLimited ? Math.max', 'return params.isRateLimited ? Math.min'],
      'private-rate-max': ['? Math.max(delayBeforeRateLimitMs', '? Math.min(delayBeforeRateLimitMs'],
    }[process.env.LIGHTER_ORACLE_MUTATION];
    if (pair) { assert.equal(source.split(pair[0]).length, 2, 'backoff mutation anchor'); source = source.replace(...pair); }
  }
  if (p.endsWith('/lighter.ws-public.ts')) {
    const pair={
      'rest-book-deep':['payload?.data?.order_book ??\n      payload?.result?.order_book ??','payload?.result?.order_book ??\n      payload?.data?.order_book ??'],
      'rest-ticker-deep':['payload?.data?.tickers,\n      payload?.result?.tickers,','payload?.result?.tickers,\n      payload?.data?.tickers,'],
      'rest-book-precedence':['payload?.order_book ??','payload?.data?.order_book ??'],
      'rest-ticker-order':['Object.values(candidate as Record<string, unknown>)','Object.values(candidate as Record<string, unknown>).reverse()'],
    }[process.env.LIGHTER_ORACLE_MUTATION];
    if(pair){assert.equal(source.split(pair[0]).length,2,'public REST source mutation anchor');source=source.replace(...pair);}
  }
  return source;
};
function load(file, imports = {}, expose = [], globals = {}) {
  const exports = {};
  const js = ts.transpileModule(read(file) + '\n' + expose.map(name => `exports.${name} = ${name};`).join('\n'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(js, { exports, URLSearchParams, ...globals, require(name) {
    assert.ok(Object.hasOwn(imports, name), `unexpected import ${name}`);
    return imports[name];
  } });
  return exports;
}
const domain = ts.createSourceFile('types.ts', read('packages/safe-cex/src/types.ts'), ts.ScriptTarget.Latest, true);
const enumExports = {};
vm.runInNewContext(ts.transpileModule(domain.statements.filter(ts.isEnumDeclaration).map(n => n.getText(domain)).join('\n'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: enumExports });
let proxy = '';
const types = load('packages/safe-cex/src/exchanges/lighter/lighter.types.ts', {
  '../../types': enumExports,
  '../../transport/registry': { withPublicCorsProxy: url => proxy ? `${proxy}/${url}` : url },
});
const ohlcv = load('packages/safe-cex/src/exchanges/lighter/lighter.ws-public-ohlcv.ts');
const deployments = [];
for (const exchangeName of [undefined, 'lighter', 'lighter-robinhood', 'binance']) {
  for (const testnet of [undefined, false, true]) for (proxy of ['', 'https://proxy.one', 'https://proxy.two/']) {
    const params = { exchangeName, testnet };
    const network = types.resolveLighterDeploymentNetwork(params);
    deployments.push({ params, proxy, robinhood: types.isLighterRobinhoodExchange(exchangeName), network,
      profile: types.resolveLighterDeploymentProfile(params), baseUrl: types.getBaseUrl(network),
      apiUrl: types.getLighterApiBaseUrl(params), wsUrl: types.getLighterWsUrl(params) });
  }
}
const timeframe = domain.statements.find(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'Timeframe');
assert.ok(timeframe && ts.isUnionTypeNode(timeframe.type));
const intervals = timeframe.type.types.map(n => {
  const interval = n.literal.text;
  try { return { interval, ok: types.resolveLighterInterval(interval) }; }
  catch (error) { return { interval, error: error.message }; }
});
const topics = [];
for (const marketId of ['0', '001', '', 'BTC/USDC', '"x', '😀', 0, -0, 1e-7, 1e21, 1e20, 1.25, -17, NaN, Infinity, -Infinity])
  for (const resolution of ['', '1m', '4h', 'x/y']) topics.push({ marketId, resolution, candle: ohlcv.buildLighterCandleTopic(marketId, resolution), trade: ohlcv.buildLighterTradesTopic(marketId) });
const recoveries = [];
for (const hasTopicHandlers of [false, true]) for (const nowMs of [-1, -0, 0, 100, 101, 1e20, NaN, Infinity, -Infinity])
  for (const lastRecoveryReconnectAtMs of [0, 1, 100, NaN, Infinity, -Infinity]) for (const recoveryCooldownMs of [-1, 0, 100, NaN, Infinity, -Infinity]) {
    const params = { hasTopicHandlers, nowMs, lastRecoveryReconnectAtMs, recoveryCooldownMs };
    recoveries.push({ ...params, expected: ohlcv.shouldAttemptLighterOhlcvRecovery(params) });
  }
const promotions = [];
for (const isWsProxyRouteEnabled of [false, true]) for (const useProxyRoute of [false, true]) for (const connectedViaProxyRoute of [false, true]) {
  const params = { isWsProxyRouteEnabled, useProxyRoute, connectedViaProxyRoute };
  promotions.push({ ...params, expected: ohlcv.shouldPromoteLighterOhlcvRecoveryToProxy(params) });
}
const parsers = load('packages/safe-cex/src/exchanges/lighter/lighter.ws-public-parsers.ts');
const dispatch = [];
for (const topic of ['ticker', 'ticker:0', 'ticker/0', 'market_stats/all', 'a:b/c', 'pong', '', '😀'])
  for (const data of ['', '{}', '{"channel":"ticker:0"}', '{"channel":"ticker/0"}', '{"channel": "ticker:0"}', '{"channel":"a:b:c"}', '{"channel":"a/b/c"}', '{"channel":"market_stats/all"}', '{"channel":"pong"}', 'not json pong', 'PONG', '{"note":"pong"}'])
    dispatch.push({ topic, data, expected: parsers.shouldDispatchLighterPublicMessage(topic, data) });
const describe = value => ({ value: value ?? null, negativeZero: Object.is(value, -0) });
const numberInputs = [null, true, false, [], {}, [1], 0, -0, 1.2, '', ' ', '\uFEFF1.5x', '\u00851', '0x10', '0b10', '+.5tail', '-0', '-.0', '1e+', '1e-3trailing', '1e999', 'Infinity', 'NaN', '+', '.', '1.2.3', '１２', { price: '123.5x' }, { price: null, p: 0, bestPrice: 5 }, { price: 'NaN', p: '1e999', best_price: '3' }];
const numbers = numberInputs.map(value => ({ values: [value, '7x'], expected: describe(parsers.parseLighterWsNumber(value, '7x')) }));
numbers.push({ values: [], expected: describe(parsers.parseLighterWsNumber()) });
const books = numberInputs.concat([[0, 1], ['2x', 3], [null, 99], [[], 1], {price:'2x'}, {p:'3x'}, {bestPrice:'4x'}, {best_price:'5x'}, {price:'1x',p:'2x',bestPrice:'3x',best_price:'4x'}, {p:'2x',bestPrice:'3x',best_price:'4x'}, {bestPrice:'3x',best_price:'4x'}, {price:'bad',p:'bad',bestPrice:'4x'}, {price:0,bestPrice:'4x'}]).map(value => ({ value, expected: describe(parsers.parseLighterWsBookSidePrice(value)) }));
const channels = ['', 'ticker:001x', 'xticker/123tail', 'TICKER:1', 'ticker:bad ticker/42', 'ticker:-1', 'ticker:１', 'ticker:1e3', '😀ticker/123', 'ticker/0'].map(channel => ({ channel, expected: parsers.extractLighterTickerMarketIdFromChannel(channel) ?? null }));

let idNow=0,idRandom=0;
const idCalls=[];
const idMath=Object.create(Math);idMath.random=()=>{idCalls.push('random');return idRandom;};
const utils = load('packages/safe-cex/src/exchanges/lighter/lighter.utils.ts', {}, ['clampDecimals','normalizeNonNegativeNumber'], {Math:idMath,Date:{now(){idCalls.push('now');return idNow;}}});
const clientIds=[];
for(idNow of [-0,0,-1,1700000000000,1e21])for(idRandom of [-0,0,Number.MIN_VALUE,2**-53,1e-7,1e-6,0.123456789,0.9999999999999999])for(const input of [{string:''},{string:'001'},{string:'-x-😀\uFEFF\u0085'},{$bigint:'0'},{$bigint:'123456789012345678901234567890'},{$bigint:'-123456789012345678901234567890'}]){
 idCalls.length=0;
 const accountIndex=Object.hasOwn(input,'string')?input.string:BigInt(input.$bigint);
 const expected=utils.generateLighterClientId(accountIndex);
 assert.deepEqual(idCalls,['now','random'],'generateLighterClientId clock/random evaluation order');
 clientIds.push({input,now:idNow,random:idRandom,expected});
}
const bigintStrings=['0','-1','123456789012345678901234567890','-123456789012345678901234567890','9'.repeat(400)].map(input=>({input,expected:utils.bigIntToString(BigInt(input))}));
const precisionInputs = ['', ' ', '\uFEFF', '\u0085', '0x10', '0b10', '0o10', '12abc', 'NaN', 'Infinity', '-Infinity', '1.200', '1.2 ', '1.2\t', '1.2\n', '1.2e+3', '1.2e-3', '1.2E-3', '1.2e-003 ', '1.2 \uFEFF', '0.1\u0085', '1e-999', '1e+999', '１２', -0, NaN, Infinity, -Infinity, 1e-7, 1e-6, 1e20, 1e21, 1.25, 0.00012];
const precision = precisionInputs.map(input => ({input,expected:utils.getPrecision(input)}));
const prefixes = ['', '0x', '0Xabc', '0x0x123', 'abc', ' 0x123', '\uFEFF0x123', '😀', '0x\n'].map(input=>({input,drop:utils.dropPrefix(input),add:utils.addPrefix(input)}));
// Double-escaped ingress/output witnesses retain valid JSON containing lone surrogates.
// sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary
const stringBoundaries=[];
for(const symbol of ['dropPrefix','addPrefix','stringToBigInt','parseLighterApiKeyIndex','getPrecision','toLighterApiKeyIndexString','parseLighterPositiveIntegerString','isLighterL1Address','normalizeNonNegativeNumber']) {
 assert.equal(typeof utils[symbol], 'function', `string boundary source function ${symbol}`);
 for(const input of ['\uD80012','\uDC0012','0x\uD800','\uD800\uD800','😀','é','abc',' \uFEFF12 ','\u008512','0x12','12']) {
  let outcome;
  try { const result=utils[symbol](input); outcome={okJson:JSON.stringify(typeof result==='bigint'?result.toString():result)??null}; }
  catch(error) { outcome={error:{name:error.name,messageUtf16:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))}}; }
  stringBoundaries.push({symbol,inputJson:JSON.stringify(input),inputUnits:Array.from({length:input.length},(_,i)=>input.charCodeAt(i)),...outcome});
 }
}
const clamps=[NaN,Infinity,-Infinity,-1e20,-30.9,-0.9,-0,0,0.9,1.1,18,29.9,30,30.1,1e20].map(input=>({input,expected:utils.clampDecimals(input)}));


const policy = load('packages/safe-cex/src/exchanges/lighter/lighter.ws-policy.ts');
const reasons=['',' ', 'unknown','geo', 'region', 'jurisdiction','restricted','policy','forbidden','auth','token','signature','permission','credential','ip','IP','xip','ipx','_ip','ip_','1ip','ip1','éipé','😀ip😀','ip\n','too many websocket messages','RATE LIMIT','auth policy','rate limit policy','rate limit auth','ſignature','apiKey'];
for (const separator of ['', '-', '_', ' ', '\t','\n','\r','\v','\f','\uFEFF','\u0085','\u00A0','\u1680','\u2000','\u2001','\u2002','\u2003','\u2004','\u2005','\u2006','\u2007','\u2008','\u2009','\u200A','\u2028','\u2029','\u202F','\u205F','\u3000','  ','--','😀']) reasons.push(`API${separator}KEY`);
for(const word of ['geo','region','jurisdiction','restricted','policy','forbidden','auth','token','signature','permission','credential']) reasons.push(`prefix${word.toUpperCase()}suffix`);
const policyPatterns=reasons.map(reason=>({reason,policy:policy.LIGHTER_POLICY_REASON_RE.test(reason),auth:policy.LIGHTER_AUTH_REASON_RE.test(reason)}));
const publicCloses=[],privateCloses=[];
for(const closeCode of [undefined,0,1000,1006,1008,1013,1015,1008.5,NaN,Infinity]) for(const closeReason of reasons) {
 publicCloses.push({closeCode:closeCode??null,closeReason,expected:policy.classifyLighterPublicClose(closeCode,closeReason)});
 for(const failedBeforeOpen of [false,true]) for(const earlyPostOpenNoStream of [false,true]) {
  const params={closeCode,closeReason,failedBeforeOpen,earlyPostOpenNoStream};privateCloses.push({...params,closeCode:closeCode??null,expected:policy.classifyLighterPrivateClose(params)});
 }
}
const policySource=ts.createSourceFile('policy.ts',read('packages/safe-cex/src/exchanges/lighter/lighter.ws-policy.ts'),ts.ScriptTarget.Latest,true);
const policySchemas=Object.fromEntries(policySource.statements.filter(n=>ts.isTypeAliasDeclaration(n)).map(n=>[n.name.text,n.type.members.map(m=>({name:m.name.text,type:m.type.getText(policySource),optional:!!m.questionToken}))]));


const builderFees=load('packages/safe-cex/src/exchanges/lighter/lighter.builder-fee.ts',{'./lighter.types':types});
const tier=load('packages/safe-cex/src/exchanges/lighter/lighter.account-tier.ts');
const approval=load('packages/safe-cex/src/exchanges/lighter/lighter.builder-approval.ts',{
 '@tealstreet/lighter-sdk':{},'@tealstreet/poseidon-crypto':{},'./lighter.account-tier':tier,'./lighter.builder-fee':builderFees,'./lighter.types':types,'./lighter.utils':utils,
},['getBaseUrl','toNumber','toBigInt','toHex10', 'approvalFeeCapsHaveNonZeroFees','getDefaultIntegratorAccountIndex']);
const approvalIntegratorDefaults=[];
for(const exchangeName of [undefined,'lighter','lighter-robinhood','binance'])for(const testnet of [false,true]){
 const params={exchangeName,testnet};
 try{approvalIntegratorDefaults.push({...params,ok:approval.getDefaultIntegratorAccountIndex(params).toString()});}
 catch(error){approvalIntegratorDefaults.push({...params,error:{name:error.name,message:error.message}});}
}
const approvalAst=ts.createSourceFile('approval.ts',read('packages/safe-cex/src/exchanges/lighter/lighter.builder-approval.ts'),ts.ScriptTarget.Latest,true);
const helperDeclaration=approvalAst.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(approvalAst)==='createLighterL1SignatureHelperUrl');
assert.ok(helperDeclaration&&ts.isArrowFunction(helperDeclaration.initializer));
const helperParams=helperDeclaration.initializer.parameters[0].type;
assert.ok(ts.isTypeLiteralNode(helperParams));
const helperUrlSchema=helperParams.members.map(m=>({name:m.name.text,type:m.type.getText(approvalAst),optional:!!m.questionToken}));
const messageDeclaration=approvalAst.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(approvalAst)==='buildLighterApproveIntegratorL1Message');
assert.ok(messageDeclaration&&ts.isArrowFunction(messageDeclaration.initializer));
const messageParams=messageDeclaration.initializer.parameters[0].type;
assert.ok(ts.isTypeLiteralNode(messageParams));
const approvalMessageSchema=messageParams.members.map(m=>({name:m.name.text,type:m.type.getText(approvalAst),optional:!!m.questionToken}));

const helperUrls=[];
const urlNumbers=[-0,0,1.5,-2,1e-7,1e20,1e21,NaN,Infinity,-Infinity];
for(let i=0;i<urlNumbers.length;i++)for(const origin of ['https://example.test','https://example.test/','','http://localhost:3000/a?q=x#frag'])for(const expectedWalletAddress of [undefined,'',' ','0x'+'a'.repeat(40),'&=+%~!*()\uFEFF\u0085😀','line\n\r\t'])for(const testnet of [false,true]){
 const params={accountIndex:-(123456789012345678901234567890n+BigInt(i)),apiKeyIndex:urlNumbers[i],approvalExpiryMs:urlNumbers[(i+1)%10],chainId:urlNumbers[(i+2)%10],expectedWalletAddress,integratorAccountIndex:123456789012345678901234567890n+BigInt(i),maxPerpsMakerFee:urlNumbers[(i+3)%10],maxPerpsTakerFee:urlNumbers[(i+4)%10],maxSpotMakerFee:urlNumbers[(i+5)%10],maxSpotTakerFee:urlNumbers[(i+6)%10],nonce:-(99999999999999999999999999n+BigInt(i)),origin,testnet};
 helperUrls.push({params:{...params,accountIndex:params.accountIndex.toString(),integratorAccountIndex:params.integratorAccountIndex.toString(),nonce:params.nonce.toString()},expected:approval.createLighterL1SignatureHelperUrl(params)});
}
const approvalTables=Object.fromEntries(Object.entries(approval).filter(([,value])=>typeof value!=='function'));
const capInputs=[0,-0,1,-1,250,NaN,Infinity,-Infinity];
const capCases=[];
for(const maxPerpsMakerFee of capInputs)for(const maxPerpsTakerFee of capInputs)for(const maxSpotMakerFee of capInputs)for(const maxSpotTakerFee of capInputs){
 const input={maxPerpsMakerFee,maxPerpsTakerFee,maxSpotMakerFee,maxSpotTakerFee};capCases.push({input,expected:approval.approvalFeeCapsHaveNonZeroFees(input)});
}
const approvalUrls=[];
for(const exchangeName of [undefined,'lighter','lighter-robinhood','binance'])for(const testnet of [false,true])for(proxy of ['', 'https://proxy'])for(const overrideBaseUrl of [undefined,'',' ','\uFEFF','\u0085','/','//','https://custom/',' https://custom/// ','\uFEFFhttps://custom/\uFEFF','https://custom/\n']){
 const params={exchangeName,testnet,overrideBaseUrl};approvalUrls.push({...params,proxy,expected:approval.getBaseUrl(params)});
}
const scalarSchemas={};
for(const [file,names] of [['lighter.utils.ts',['LighterCancelTarget','LighterOrderMinimums']],['lighter.builder-approval.ts',['LighterBuilderApprovalFeeCaps']]]){
 const ast=ts.createSourceFile(file,read('packages/safe-cex/src/exchanges/lighter/'+file),ts.ScriptTarget.Latest,true);
 for(const name of names){const alias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);assert.ok(alias&&ts.isTypeLiteralNode(alias.type));scalarSchemas[name]=alias.type.members.map(m=>({name:m.name.text,type:m.type.getText(ast),optional:!!m.questionToken}));}
}

const approvalNumbers=[undefined,null,false,true,0,-0,1,-1,1e-7,1e21,'',' ','\uFEFF0x10\uFEFF','\u008516','0x10','0b10','0o10','12abc','12e2','Infinity','-Infinity','NaN','1e400',[],[null],[12],['12'],[1,2],{}, {valueOf:1}, {toString:null},[{toString:0}]].map(input=>{
 try{return {input,ok:approval.toNumber(input)??null};}catch(error){return {input,error:{name:error.name,message:error.message}};}
});
const bigintParseInputs=['',' ','\uFEFF42\uFEFF','\u008542','0','-0','+0','0012','-12','+12','0x10','0Xff','0o10','0b10','-0x10','+0x10','0x','1.0','1e2','12abc','NaN','Infinity','１２','😀','9'.repeat(400),'x'.repeat(1001),'x'.repeat(999)+'😀'];
for(let i=0;i<400;i++)bigintParseInputs.push(`${['','-','+','0x','0b','\uFEFF'][i%6]}${i}${['','x','.0','\u0085'][i%4]}`);
const bigintParses=bigintParseInputs.map(input=>{try{return {input,ok:utils.stringToBigInt(input).toString()};}catch(error){return {input,error:{name:error.name,messageUtf16:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))}};}});
const approvalBigintInputs=[undefined,null,false,true,0,-0,1,-1,0.1,-0.1,1e20,1e21,1e-7,NaN,Infinity,-Infinity,0n,-1n,1n,2n**64n-1n,2n**64n,10n**400n,...bigintParseInputs,[],[null],[12],['12'],[1,2],{}, {valueOf:1},{toString:null},[{toString:0}]];
const approvalBigints=approvalBigintInputs.map(input=>({input,expected:approval.toBigInt(input)?.toString()??null}));
const hexInputs=[-0,0,-1,1,-0.1,0.1,NaN,Infinity,-Infinity,2**53-1,2**53,2**64-2048,2**64,Number.MAX_VALUE,-1n,0n,1n,2n**53n-1n,2n**53n+1n,2n**64n-1n,2n**64n,10n**100n];
for(let i=0;i<64;i++){hexInputs.push(2**i,2n**BigInt(i)-1n,2n**BigInt(i)+1n);}
const approvalHex=[];
for(const input of hexInputs)for(const field of ['nonce','', '😀\uFEFFfield']) {
 try{approvalHex.push({input,field,ok:approval.toHex10(input,field)});}catch(error){approvalHex.push({input,field,error:{name:error.name,message:error.message,messageUtf16:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))}});}
}
const scaledInts=[];
for(const value of [NaN,Infinity,-Infinity,0,-0,0.5,-0.5,0.49999999999999994,0.5000000000000001,-0.49999999999999994,-0.5000000000000001,1.5,-1.5,0.25,-0.25,0.1,-0.1,1e-30,-1e-30,Number.MIN_VALUE,-Number.MIN_VALUE,Number.MAX_VALUE,-Number.MAX_VALUE,9007199254740991,9007199254740992])for(const decimals of [NaN,Infinity,-Infinity,-1,-0,0,0.5,1,2,6,18,29,30,31,100,...Array.from({length:31},(_,i)=>i)]) {
 try{scaledInts.push({value,decimals,ok:utils.toLighterScaledInt(value,decimals).toString()});}catch(error){scaledInts.push({value,decimals,error:{name:error.name,message:error.message,messageUtf16:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))}});}
}
const approvalMessageBase={nonce:1n,accountIndex:2n,apiKeyIndex:3,integratorAccountIndex:4n,maxPerpsTakerFee:5,maxPerpsMakerFee:6,maxSpotTakerFee:7,maxSpotMakerFee:8,approvalExpiryMs:9n,chainId:304};
const approvalMessageCases=[];
const addMessage=params=>{try{approvalMessageCases.push({params,ok:approval.buildLighterApproveIntegratorL1Message(params)});}catch(error){approvalMessageCases.push({params,error:{name:error.name,message:error.message,messageUtf16:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))}});}};
addMessage(approvalMessageBase);
for(const [key,value] of Object.entries(approvalMessageBase)) {
 const inputs=typeof value==='bigint'?[-1n,0n,2n**53n+1n,2n**64n-1n,2n**64n]:[-1,-0,0,0.5,NaN,Infinity,-Infinity,2**53,2**64,466324,300];
 for(const input of inputs)addMessage({...approvalMessageBase,[key]:input});
}
const messageKeys=Object.keys(approvalMessageBase);
for(const left of messageKeys)for(const right of messageKeys){if(left===right)continue;addMessage({...approvalMessageBase,[left]:typeof approvalMessageBase[left]==='bigint'?-1n:-1,[right]:typeof approvalMessageBase[right]==='bigint'?2n**64n:0.5});}
const publicAst=ts.createSourceFile('public.ts',read('packages/safe-cex/src/exchanges/lighter/lighter.ws-public.ts'),ts.ScriptTarget.Latest,true);
const fundingDeclaration=publicAst.statements.filter(ts.isVariableStatement).find(s=>s.declarationList.declarations.some(d=>d.name.getText(publicAst)==='normalizeLighterFundingRate'));
assert.ok(fundingDeclaration,'normalizeLighterFundingRate source');
const fundingExports={};
vm.runInNewContext(ts.transpileModule(fundingDeclaration.getText(publicAst)+'\nexports.normalize=normalizeLighterFundingRate;', {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:fundingExports});
const publicClass=publicAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='LighterPublicWebsocket');
const publicRestNames=['parseRestOrderBookLevels','parseRestTickerItems'];
const restDeclarations=publicRestNames.map(name=>{const member=publicClass.members.find(n=>n.name?.getText(publicAst)===name);assert.ok(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),name);return `exports.${name}=${member.initializer.getText(publicAst)};`;});
vm.runInNewContext(ts.transpileModule(restDeclarations.join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:fundingExports});
const restPayloadInputs=[undefined,null,false,0,'text',[],[1,{symbol:'😀'}],{}, {bids:[[1,2]],asks:[]},{asks:[null,'x']},{bids:'wrong',asks:[0]}, {bids:[],asks:[]}];
const restPaths=['order_book','data.order_book','result.order_book','orderbook','data.orderbook','result.orderbook','order_book_stats','data.order_book_stats','result.order_book_stats','tickers','data.tickers','result.tickers','market_stats','data.market_stats','result.market_stats','data','result'];
for(const path of restPaths)for(const value of [null,false,0,'text',[],[1,2],{}, {bids:[],asks:[]},{bids:[[1,2]],asks:[[3,4]]}, {z:'z','2':'two','01':'leading','1':'one',a:'a'}, {symbol:'😀\uFEFF\u0085'}]) {
 const payload={order_book:{bids:[['first',1]]},data:{tickers:[{symbol:'fallback'}]}};
 let target=payload;const parts=path.split('.');for(const part of parts.slice(0,-1)){target[part]??={};target=target[part];}target[parts.at(-1)]=value;
 restPayloadInputs.push(payload);
 const standalone={};target=standalone;for(const part of parts.slice(0,-1)){target[part]={};target=target[part];}target[parts.at(-1)]=value;restPayloadInputs.push(standalone);
}
// Every alias pair competes, not only the first alias versus a fallback.
function setRestPath(target,path,value){const parts=path.split('.');for(const key of parts.slice(0,-1)){if(!target[key]||typeof target[key]!=='object'||Array.isArray(target[key]))target[key]={};target=target[key];}target[parts.at(-1)]=value;}
for(let first=0;first<restPaths.length;first++)for(let second=first+1;second<restPaths.length;second++)for(const shape of ['book','ticker']) {
 const early=shape==='book'?{bids:[['early',1]],asks:[['early-ask',2]]}:[{symbol:'early'}];
 const late=shape==='book'?{bids:[['late',3]],asks:[['late-ask',4]]}:[{symbol:'late'}];
 for(const firstValue of [early,null,false,0,'mask',{},[]]) {
  const payload={};setRestPath(payload,restPaths[first],firstValue);setRestPath(payload,restPaths[second],late);restPayloadInputs.push(payload);
 }
}
const collectionIdentityInputs=require('./collection-identity.cjs').publicInputs;
assert.equal(collectionIdentityInputs.length,8,'public collection identity inventory');
restPayloadInputs.unshift(...collectionIdentityInputs);
const publicRestPayloads=restPayloadInputs.map(input=>({input,book:fundingExports.parseRestOrderBookLevels(input)??null,tickers:fundingExports.parseRestTickerItems(input)}));
const publicRestBoundaries=['\uD800','\uDC00','\uD80012'].map(symbol=>{const input={order_book:{bids:[symbol]},order_book_stats:[{symbol}]};return {inputJson:JSON.stringify(input),bookJson:JSON.stringify(fundingExports.parseRestOrderBookLevels(input)),tickersJson:JSON.stringify(fundingExports.parseRestTickerItems(input))};});
const fundingInputs=[NaN,Infinity,-Infinity,0,-0,Number.MIN_VALUE,-Number.MIN_VALUE,Number.MAX_VALUE,-Number.MAX_VALUE,0.0012,-0.0012,0.0013,1,-1];
const fundingView=new DataView(new ArrayBuffer(8));fundingView.setFloat64(0,0.0012);const floorBits=fundingView.getBigUint64(0);
for(let i=-256n;i<=256n;i++){fundingView.setBigUint64(0,floorBits+i);fundingInputs.push(fundingView.getFloat64(0));}
let fundingSeed=0x12345678;
for(let i=0;i<2000;i++){fundingSeed=(Math.imul(fundingSeed,1664525)+1013904223)>>>0;fundingView.setUint32(0,fundingSeed);fundingSeed=(Math.imul(fundingSeed,1664525)+1013904223)>>>0;fundingView.setUint32(4,fundingSeed);fundingInputs.push(fundingView.getFloat64(0));}
const fundingRates=fundingInputs.map(input=>({input,expected:fundingExports.normalize(input)}));
const backoffValues=[NaN,Infinity,-Infinity,-0,0,-1,0.5,1,8,9,2000,Number.MAX_VALUE,Number.MIN_VALUE,1e-7];
const exponentialBackoffs=[];
const addBackoff=(baseDelayMs,failureCount,opts)=>exponentialBackoffs.push({baseDelayMs,failureCount,opts,expected:policy.calculateExponentialBackoffDelay(baseDelayMs,failureCount,opts)});
for(const base of backoffValues)for(const count of backoffValues){addBackoff(base,count,undefined);for(const opts of [{},{multiplier:0},{multiplier:-0},{multiplier:-1,maxExponent:Infinity,maxDelayMs:Infinity},{multiplier:NaN},{maxExponent:NaN},{maxDelayMs:NaN},{maxDelayMs:-0},{maxDelayMs:0}])addBackoff(base,count,opts);}
for(const multiplier of backoffValues)for(const maxExponent of backoffValues)for(const maxDelayMs of [-0,0,Infinity,NaN])addBackoff(1,0.5,{multiplier,maxExponent,maxDelayMs});
const publicBackoffs=[],privateBackoffs=[];
for(const baseDelayMs of backoffValues)for(const failureCount of backoffValues)for(const rateLimitReconnectDelayMs of backoffValues)for(const isRateLimited of [false,true]){
 const params={baseDelayMs,failureCount,rateLimitReconnectDelayMs,isRateLimited};publicBackoffs.push({...params,expected:policy.calculateLighterPublicReconnectDelay(params)});
 for(const shouldBackoff of [false,true])for(const isWsRateLimited of [false,true]){const p={baseDelayMs,failedHandshakes:failureCount,rateLimitReconnectDelayMs,isRateLimited,shouldBackoff,isWsRateLimited};privateBackoffs.push({...p,expected:policy.calculateLighterPrivateReconnectDelay(p)});}
}
const backoffSchemas={};
for(const [name,index] of [['calculateExponentialBackoffDelay',2],['calculateLighterPublicReconnectDelay',0],['calculateLighterPrivateReconnectDelay',0]]){
 const decl=policySource.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name.text===name);assert.ok(decl,'backoff declaration');const type=decl.parameters[index].type;assert.ok(ts.isTypeLiteralNode(type),'backoff parameter object');backoffSchemas[name]=type.members.map(m=>({name:m.name.text,type:m.type.getText(policySource),optional:!!m.questionToken}));
}
if(process.env.LIGHTER_PURE_CARDINALITY_MUTATION==='empty') publicRestPayloads.splice(0);
if(process.env.LIGHTER_PURE_CARDINALITY_MUTATION==='subset') publicRestPayloads.splice(1);
process.stdout.write(JSON.stringify({ publicRestPayloads, publicRestBoundaries, stringBoundaries, exponentialBackoffs, publicBackoffs, privateBackoffs, backoffSchemas, fundingRates, approvalMessageSchema, approvalMessageCases, scaledInts, approvalBigints, approvalHex, bigintParses, approvalNumbers, clientIds, bigintStrings, helperUrlSchema, helperUrls, approvalIntegratorDefaults, approvalTables, capCases, approvalUrls, scalarSchemas, policyPatterns, publicCloses, privateCloses, policySchemas, precision, prefixes, clamps, deployments, intervals, topics, recoveries, promotions, dispatch, numbers, books, channels }, (_key, value) => {
  if (typeof value === 'number' && (!Number.isFinite(value) || Object.is(value, -0))) return { $number: Object.is(value, -0) ? '-0' : String(value) };
  if (typeof value === 'bigint') return {$bigint:value.toString()};
  return value;
}));
