const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types');
const mutation = process.env.LIGHTER_SDK_BATCHES_MUTATION;
const sources = Object.fromEntries(['transactions', 'orders'].map(name => [path.join(root, `${name}.ts`), fs.readFileSync(path.join(root, `${name}.ts`), 'utf8')]));
for (const [file, original] of Object.entries(sources)) {
  let source = original;
  for (const name of ['BatchTxReq', 'BatchTxResponse', 'BatchCreateOrdersParams']) {
    if (mutation === `heritage-${name}`) source = source.replace(`interface ${name} {`, `interface ${name} extends ExtraNumeric {`) + '\ninterface ExtraNumeric { hiddenNumeric: number; }';
  }
  if (mutation === 'transaction-union') source = source.replace('CreateOrderTxReq | CancelOrderTxReq | TransferTxReq | WithdrawTxReq', 'CreateOrderTxReq | CancelOrderTxReq | TransferTxReq');
  if (mutation === 'order-union') source = source.replace('CreateLimitOrderParams | CreateMarketOrderParams', 'CreateLimitOrderParams');
  if (mutation === 'response-index-string') source = source.replace('successIndices: number[];', 'successIndices: string[];');
  if (mutation === 'missing-import') {
    const imports = source.match(/^import \{[^\n]+\} from '\.\/constants';$/gm);
    assert.equal(imports?.length,1,'missing-import control constants import count');
    source = source.replace(imports[0],'');
  }
  sources[file] = source;
}
const create = {symbol:'CREATE',type:0,side:1,price:'11.25',quantity:'12.5',timeInForce:2,reduceOnly:false,postOnly:true,stopPrice:'13.75',clientOrderId:'create-id',nonce:21.25,expiry:22.5,accountIndex:23.75,apiKeyIndex:24.125,extra:{left:91,right:92}};
const cancel = {symbol:'CANCEL',orderId:'cancel-id',nonce:31.25,expiry:32.5,accountIndex:33.75,apiKeyIndex:34.125,extra:{left:93,right:94}};
const transfer = {token:'TRANSFER',recipient:'recipient',amount:'41.25',memo:'memo',nonce:42.5,expiry:43.75,accountIndex:44.125,apiKeyIndex:45.5,extra:{left:95,right:96}};
const withdraw = {token:'WITHDRAW',amount:'51.25',l1Recipient:'l1-recipient',fastWithdraw:true,nonce:52.5,expiry:53.75,accountIndex:54.125,apiKeyIndex:55.5,extra:{left:97,right:98}};
const limit = {symbol:'LIMIT',side:0,price:'61.25',quantity:'62.5',timeInForce:1,postOnly:true,reduceOnly:false,clientOrderId:'limit-id',accountIndex:63.75,apiKeyIndex:64.125,expiry:65.5,extra:{left:99,right:100}};
const market = {symbol:'MARKET',side:1,quantity:'71.25',reduceOnly:true,clientOrderId:'market-id',accountIndex:72.5,apiKeyIndex:73.75,extra:{left:101,right:102}};
const cases = {
  BatchTxReq: [
    {name:'all-four',input:{transactions:[create,cancel,transfer,withdraw],accountIndex:101.25,apiKeyIndex:102.5}},
    {name:'create-cancel-overlap',input:{transactions:[{...create,orderId:'overlap-cancel'}]}},
    {name:'transfer-withdraw-overlap',input:{transactions:[{...transfer,l1Recipient:'overlap-l1',fastWithdraw:false}]}},
    {name:'invalid-child',input:{transactions:[{symbol:'INVALID'}]}},
    {name:'empty',input:{transactions:[]}},
  ],
  BatchCreateOrdersParams: [
    {name:'both-orders',input:{orders:[limit,market],accountIndex:201.25,apiKeyIndex:202.5}},
    {name:'limit-valid-market',input:{orders:[limit]}},
    {name:'malformed-limit-valid-market',input:{orders:[{...market,price:123,postOnly:null,timeInForce:'bad',expiry:'bad'}]}},
    {name:'invalid-child',input:{orders:[{symbol:'INVALID'}]}},
    {name:'empty',input:{orders:[]}},
  ],
  BatchTxResponse: [
    {name:'distinct',input:{txHashes:['hash-B','hash-A','hash-B'],successIndices:[2.25,4.5],failedIndices:[7.75],errors:{'2':'two','10':'ten',alpha:'alpha-error'}}},
    {name:'empty',input:{txHashes:[],successIndices:[],failedIndices:[],errors:{}}},
    {name:'nonnumeric-error-value',input:{txHashes:[],successIndices:[],failedIndices:[],errors:{foo:123}}},
    {name:'numeric-error-value',input:{txHashes:[],successIndices:[],failedIndices:[],errors:{'1':123}}},
  ],
};
if (mutation === 'discriminating-content') cases.BatchTxReq[0].input.transactions[0].quantity = '12.75';
const names = ['BatchTxReq','BatchTxResponse','BatchCreateOrdersParams','CreateOrderTxReq','CancelOrderTxReq','TransferTxReq','WithdrawTxReq','CreateLimitOrderParams','CreateMarketOrderParams'];
const probes = [];
for (const [record, rows] of Object.entries(cases)) for (const row of rows) probes.push({name:`probe${probes.length}`,record,row});
const probeFile = path.join(root, '__batch_cardinality_probe__.ts');
const childProbes = probes.flatMap(p => (p.row.input.transactions ?? p.row.input.orders ?? []).map((input,index)=>({name:`${p.name}_child${index}`,input})));
const nonJsonProbes = ['BatchTxReq','BatchCreateOrdersParams'].flatMap((record,recordIndex)=>[
  ['undefined','undefined'],['bigint','17n'],['function','() => 17'],['symbol',"Symbol('batch-extra')"],
].map(([kind,expression],index)=>({record,kind,expression,name:`nonJson${recordIndex}_${index}`})));
const probeText = [
  ...probes.map(p => `const ${p.name} = ${JSON.stringify(p.row.input)};`),
  ...childProbes.map(p=>`const ${p.name} = ${JSON.stringify(p.input)};`),
  ...nonJsonProbes.map(p=>`const ${p.name} = {${p.record==='BatchTxReq'?'transactions':'orders'}:[{...${JSON.stringify(p.record==='BatchTxReq'?create:limit)},extra:${p.expression}}]};`),
  `const nonJsonCapture = [${nonJsonProbes.map(p=>p.name).join(',')}];`,
].join('\n');
const host = ts.createCompilerHost({});
const read = host.readFile.bind(host), exists = host.fileExists.bind(host);
host.readFile = file => file === probeFile ? probeText : sources[file] ?? read(file);
host.fileExists = file => file === probeFile || exists(file);
const program = ts.createProgram([...Object.keys(sources),probeFile], {target:ts.ScriptTarget.ES2022,strictNullChecks:true,skipLibCheck:true}, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length,0,`batch source program diagnostics must be empty: ${diagnostics.length}\n${ts.formatDiagnostics(diagnostics,{getCanonicalFileName:file=>file,getCurrentDirectory:()=>root,getNewLine:()=> '\n'})}`);
const checker = program.getTypeChecker();
const declarations = {};
for (const file of Object.keys(sources)) {
  const tree = program.getSourceFile(file);
  for (const node of tree.statements.filter(ts.isInterfaceDeclaration)) if (names.includes(node.name.text)) {
    assert.ok(!node.heritageClauses?.length, `${node.name.text} unsupported interface heritage`);
    declarations[node.name.text] = node;
  }
}
const schemas = Object.fromEntries(names.map(name => {
  const node = declarations[name]; assert.ok(node, `${name} source declaration`);
  return [name,node.members.map(field => ({name:field.name.text,type:field.type.getText(node.getSourceFile()),optional:!!field.questionToken}))];
}));
const expectedRoots = {
  BatchTxReq:[['transactions','(CreateOrderTxReq | CancelOrderTxReq | TransferTxReq | WithdrawTxReq)[]',false],['accountIndex','number',true],['apiKeyIndex','number',true]],
  BatchCreateOrdersParams:[['orders','(CreateLimitOrderParams | CreateMarketOrderParams)[]',false],['accountIndex','number',true],['apiKeyIndex','number',true]],
  BatchTxResponse:[['txHashes','string[]',false],['successIndices','number[]',false],['failedIndices','number[]',false],['errors','Record<number, string>',false]],
};
const optionalIndexes = [['nonce','number',true],['expiry','number',true],['accountIndex','number',true],['apiKeyIndex','number',true]];
const expectedChildren = {
  CreateOrderTxReq:[['symbol','string',false],['type','OrderType',false],['side','OrderSide',false],['price','string',false],['quantity','string',false],['timeInForce','TimeInForce',true],['reduceOnly','boolean',true],['postOnly','boolean',true],['stopPrice','string',true],['clientOrderId','string',true],...optionalIndexes],
  CancelOrderTxReq:[['symbol','string',false],['orderId','string',false],...optionalIndexes],
  TransferTxReq:[['token','string',false],['recipient','string',false],['amount','string',false],['memo','string',true],...optionalIndexes],
  WithdrawTxReq:[['token','string',false],['amount','string',false],['l1Recipient','string',true],['fastWithdraw','boolean',true],...optionalIndexes],
  CreateLimitOrderParams:[['symbol','string',false],['side','OrderSide',false],['price','string',false],['quantity','string',false],['timeInForce','TimeInForce',true],['postOnly','boolean',true],['reduceOnly','boolean',true],['clientOrderId','string',true],['accountIndex','number',true],['apiKeyIndex','number',true],['expiry','number',true]],
  CreateMarketOrderParams:[['symbol','string',false],['side','OrderSide',false],['quantity','string',false],['reduceOnly','boolean',true],['clientOrderId','string',true],['accountIndex','number',true],['apiKeyIndex','number',true]],
};
for (const [name, fields] of Object.entries({...expectedRoots,...expectedChildren})) assert.deepEqual(schemas[name].map(f=>[f.name,f.type,f.optional]),fields,`${name} source schema`);
const unions = {};
for (const [record,field,expected] of [
  ['BatchTxReq','transactions',['CreateOrderTxReq','CancelOrderTxReq','TransferTxReq','WithdrawTxReq']],
  ['BatchCreateOrdersParams','orders',['CreateLimitOrderParams','CreateMarketOrderParams']],
]) {
  const type = declarations[record].members.find(f=>f.name.text===field).type;
  assert.ok(ts.isArrayTypeNode(type) && ts.isParenthesizedTypeNode(type.elementType) && ts.isUnionTypeNode(type.elementType.type),`${record}.${field} source union array`);
  const members = type.elementType.type.types.map(t=>{assert.ok(ts.isTypeReferenceNode(t),`${record}.${field} named union member`);return t.typeName.getText();});
  assert.deepEqual(members,expected,`${record}.${field} union members`);unions[record]=members;
}
const probeNodes = program.getSourceFile(probeFile).statements.map(s=>s.declarationList.declarations[0]);
for (const [index,p] of probes.entries()) {
  const sourceType = checker.getTypeAtLocation(probeNodes[index].name);
  p.row.accepted = checker.isTypeAssignableTo(sourceType,checker.getTypeAtLocation(declarations[p.record]));
  if (unions[p.record]) {
    const key = p.record==='BatchTxReq'?'transactions':'orders';
    p.row.matches = p.row.input[key].map((_,childIndex)=>{
      const child = probeNodes.find(node=>node.name.text===`${p.name}_child${childIndex}`);
      return unions[p.record].filter(name=>checker.isTypeAssignableTo(checker.getTypeAtLocation(child.name),checker.getTypeAtLocation(declarations[name])));
    });
  }
}
const capturedNonJson = new Function(ts.transpileModule(probeText,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+'\nreturn nonJsonCapture;')();
const nonJsonExtras = nonJsonProbes.map((probe,index)=>{
  const node=probeNodes.find(node=>node.name.text===probe.name);
  const accepted=checker.isTypeAssignableTo(checker.getTypeAtLocation(node.name),checker.getTypeAtLocation(declarations[probe.record]));
  const child=capturedNonJson[index][probe.record==='BatchTxReq'?'transactions':'orders'][0];
  const witness={record:probe.record,kind:probe.kind,accepted,ownPresent:Object.hasOwn(child,'extra'),typeof:typeof child.extra,expression:probe.expression};
  assert.equal(witness.accepted,true,`${probe.record} ${probe.kind} extra checker admission`);
  assert.equal(witness.ownPresent,true,`${probe.record} ${probe.kind} own extra retained`);
  assert.equal(witness.typeof,probe.kind,`${probe.record} ${probe.kind} extra observed before serialization`);
  return witness;
});
assert.deepEqual(cases.BatchTxReq.map(c=>[c.name,c.accepted]),[['all-four',true],['create-cancel-overlap',true],['transfer-withdraw-overlap',true],['invalid-child',false],['empty',true]],'BatchTxReq checker admission');
assert.deepEqual(cases.BatchCreateOrdersParams.map(c=>[c.name,c.accepted]),[['both-orders',true],['limit-valid-market',true],['malformed-limit-valid-market',true],['invalid-child',false],['empty',true]],'BatchCreateOrdersParams checker admission');
assert.deepEqual(cases.BatchTxResponse.map(c=>[c.name,c.accepted]),[['distinct',true],['empty',true],['nonnumeric-error-value',true],['numeric-error-value',false]],'BatchTxResponse numeric-key checker admission');
assert.deepEqual(cases.BatchTxReq[1].matches,[['CreateOrderTxReq','CancelOrderTxReq']],'create/cancel structural overlap');
assert.deepEqual(cases.BatchTxReq[2].matches,[['TransferTxReq','WithdrawTxReq']],'transfer/withdraw structural overlap');
assert.deepEqual(cases.BatchCreateOrdersParams[1].matches,[['CreateLimitOrderParams','CreateMarketOrderParams']],'limit/market structural overlap');
assert.deepEqual(cases.BatchCreateOrdersParams[2].matches,[['CreateMarketOrderParams']],'invalid limit-only fields remain market extras');
assert.deepEqual(cases.BatchTxReq[0].input.transactions.map(x=>x.symbol??x.token),['CREATE','CANCEL','TRANSFER','WITHDRAW'],'transaction child identities');
assert.deepEqual(cases.BatchCreateOrdersParams[0].input.orders.map(x=>[x.symbol,x.side,x.quantity,x.reduceOnly]),[['LIMIT',0,'62.5',false],['MARKET',1,'71.25',true]],'order child identities');
assert.deepEqual([cases.BatchTxResponse[0].input.txHashes,cases.BatchTxResponse[0].input.successIndices,cases.BatchTxResponse[0].input.failedIndices],[['hash-B','hash-A','hash-B'],[2.25,4.5],[7.75]],'response independent contents');
const base = Object.fromEntries(Object.entries(cases).map(([name,rows])=>[name,rows[0].input]));
const identitySha256 = createHash('sha256').update(JSON.stringify(cases)).digest('hex');
assert.equal(identitySha256,'e4a67a59eae708a9b14e37450fa04bc58c6ff0c51040f83fc8193b07060e2c62','batch independent complete fixture contents');
const set = (input,keys,value) => {let cursor=input;for(const key of keys.slice(0,-1))cursor=cursor[key];cursor[keys.at(-1)]=value;};
const get = (input,keys) => keys.reduce((value,key)=>value[key],input);
const numericPaths = [];
for (const record of ['BatchTxReq','BatchCreateOrdersParams']) {
  for (const field of ['accountIndex','apiKeyIndex']) numericPaths.push({record,path:[field]});
  const key = record==='BatchTxReq'?'transactions':'orders';
  const children = record==='BatchTxReq'?['CreateOrderTxReq','CancelOrderTxReq','TransferTxReq','WithdrawTxReq']:['CreateLimitOrderParams','CreateMarketOrderParams'];
  children.forEach((child,index)=>schemas[child].filter(f=>['number','OrderType','OrderSide','TimeInForce'].includes(f.type)).forEach(f=>numericPaths.push({record,path:[key,index,f.name]})));
}
for (const field of ['successIndices','failedIndices']) base.BatchTxResponse[field].forEach((_,index)=>numericPaths.push({record:'BatchTxResponse',path:[field,index]}));
const lexemes = ['-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25','9007199254740993'];
const bits = number => {if(Number.isNaN(number))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(number);return bytes.toString('hex');};
const numeric = numericPaths.flatMap(({record,path})=>lexemes.map(raw=>{
  const input=structuredClone(base[record]);set(input,path,'__NUMBER__');const encoded=JSON.stringify(input).replace('"__NUMBER__"',raw);
  return {record,path,raw,encoded,bits:bits(get(JSON.parse(encoded),path))};
}));
const stringPaths = [
  ['BatchTxReq',['transactions',0,'symbol']],['BatchTxReq',['transactions',2,'memo']],
  ['BatchCreateOrdersParams',['orders',0,'symbol']],['BatchCreateOrdersParams',['orders',1,'clientOrderId']],
  ['BatchTxResponse',['txHashes',0]],['BatchTxResponse',['errors','alpha']],
  ...Object.keys(base).map(record=>[record,['extra']]),
];
const strings = stringPaths.flatMap(([record,path])=>['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
  const input=structuredClone(base[record]);set(input,path,text);const encoded=JSON.stringify(input);assert.equal(get(JSON.parse(encoded),path),text);
  return {record,path,encoded,units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),representable:text.isWellFormed()};
}));
const depth = Object.keys(base).flatMap(record=>['root',...(record==='BatchTxResponse'?[]:['child'])].flatMap(location=>[0,2,123,124,125,126,127,140].map(depth=>{
  const input=structuredClone(base[record]);let extra=17;for(let i=0;i<depth;i++)extra=[extra];
  const keys=location==='root'?['extra']:[record==='BatchTxReq'?'transactions':'orders',0,'extra'];set(input,keys,extra);
  const encoded=JSON.stringify(input);let value=get(JSON.parse(encoded),keys),count=0;while(Array.isArray(value)){assert.equal(value.length,1);count++;value=value[0];}assert.equal(count,depth);assert.equal(value,17);
  return {record,location,depth,encoded};
})));
const optionalNull = ['BatchTxReq','BatchCreateOrdersParams'].flatMap(record=>['accountIndex','apiKeyIndex'].map(field=>({record,field,input:{...base[record],[field]:null}})));
const retainedExtras = ['BatchTxReq','BatchCreateOrdersParams'].flatMap(record=>{
  const field=record==='BatchTxReq'?'transactions':'orders';return base[record][field].map((child,index)=>({record,path:[field,index,'extra'],expected:child.extra}));
});
const errorKeyProbes = cases.BatchTxResponse.slice(2).map(row=>({key:Object.keys(row.input.errors)[0],value:Object.values(row.input.errors)[0],accepted:row.accepted}));
process.stdout.write(JSON.stringify({header:{nodeVersion:process.version,captureArchitecture:process.arch,capturePlatform:process.platform,consumer:'live_sdk_batches',scope:'later source-checked batch record supplement',identitySha256},schemas,unions,cases,numeric,strings,depth,optionalNull,retainedExtras,errorKeyProbes,nonJsonExtras}));
