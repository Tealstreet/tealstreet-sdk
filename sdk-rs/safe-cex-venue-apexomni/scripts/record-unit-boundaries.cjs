// Witness only declared String leaves; never invent an undeclared-extra carrier.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
function api(name,expose){
 const filename=path.resolve(__dirname,name);let code=fs.readFileSync(filename,'utf8');
 const ast=ts.createSourceFile(filename,code,ts.ScriptTarget.Latest,true,ts.ScriptKind.JS);
 const emit=ast.statements.filter(n=>ts.isExpressionStatement(n)&&ts.isCallExpression(n.expression)&&['process.stdout.write','console.log'].includes(n.expression.expression.getText(ast)));
 assert.equal(emit.length,1,'APEX-RECORD-UNIT maintained emitter selector');
 code=code.slice(0,emit[0].getStart(ast))+expose+code.slice(emit[0].end);
 const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);return m.exports;
}
const plain=api('records.cjs','module.exports={records};').records;
const transactions=api('transaction-records.cjs','module.exports={schemas,sample};');
function txShape(shape,stack=[]){
 if(shape.kind==='record'){assert(!stack.includes(shape.name));return txRecord(shape.name,[...stack,shape.name]);}
 if(shape.kind==='union')return {kind:'union',choices:shape.items.map(x=>txShape(x,stack))};
 if(shape.kind==='literal')return {kind:'literal',value:shape.values[0]};
 if(shape.kind==='tuple')return {kind:'tuple',items:shape.items.map(x=>txShape(x,stack))};
 return shape;
}
function txRecord(name,stack=[]){return {kind:'record',fields:transactions.schemas[name].map(f=>({name:f.name,optional:f.optional,type:txShape(f.shape,stack)}))};}
const targets=[
 ['KeyPair','publicKey'],['ISO8601',''],['ApiKeyCredentials','key'],['WithPrice','humanPrice'],['OrderWithClientId','positionId'],
 ['UserObject','ethereumAddress'],['WalletObject','userId'],['OpenPositionObject','userId'],['AccountsItem','takerFeeRate'],['AccountObject','starkKey'],['OrderObject','id'],['WorstPriceObject','worstPrice'],['CreateOrderOptionsObject','clientOrderId'],['FundingRateObject','id'],['HistoricalPNLObject','symbol'],['OrderResponseObject','id'],['AccountBalanceObject','totalEquityValue'],
 ['TickerObject','fundingRate'],['DepthObject','a.0.0'],['GetHistoryFundingOptions','symbol'],['PerpetualCurrencyObject','usdc.0.id'],['CurrencyObject','id'],['GlobalObject','feeAccountId'],['PerpetualContractObject','baselinePositionValue'],['MultiChainObject','currency'],['PerpetualObject','usdcConfig.currency.0.id'],['SymbolObject','currency.0.id'],['TradeObject','i'],['KlineObject','s'],['HistoryFundingObject','symbol'],['SymbolInfoObject','baseCoin'],
 ['ChangePubKeyECDSA','ethSignature'],['ChangePubKeyCREATE2','creatorAddress'],['SignChangePubKeyPayload','fee'],['TxChangePubKeyData','account'],['SignedChangePubKeyData','tx.account'],
 ['SignContractPayload','size'],['TxContractData','size'],['SignedContractData','tx.size'],['SignOrderPayload','amount'],['TxOrderData','amount'],['SignedOrderData','tx.amount'],['SignTransferPayload','to'],['TxTransferData','to'],['SignedTransferData','tx.to'],['SignWithdrawPayload','to'],['TxWithdrawData','from'],['SignedWithdrawData','tx.from'],['TxEthSignature','signature'],['TxSignature','pubKey'],
];
assert.equal(targets.length,50,'APEX-RECORD-UNIT independent targeted row floor');
function chosen(schema){return schema.kind==='union'?chosen(schema.choices.find(s=>!['undefined','null'].includes(s.kind))):schema;}
function sample(schema,seed=1){
 schema=chosen(schema);
 switch(schema.kind){
 case 'string':return 'field-'+seed;case 'number':return seed+.25;case 'boolean':return true;case 'literal':return schema.value;
 case 'null':return null;case 'any':return {opaque:'value',number:seed+.25};
 case 'array':return [sample(schema.item,seed),sample(schema.item,seed+100)];
 case 'tuple':return schema.items.map((s,i)=>sample(s,seed+i));
 case 'record':return Object.fromEntries(schema.fields.filter(f=>f.type.kind!=='undefined').map((f,i)=>[f.name,sample(f.type,seed+i)]));
 default:throw Error('APEX-RECORD-UNIT unsupported actual schema '+schema.kind);
 }
}
function leaf(schema,keys){
 schema=chosen(schema);
 if(!keys.length)return schema;
 const [key,...rest]=keys;
 if(schema.kind==='array'){assert.equal(key,'0');return leaf(schema.item,rest);}
 if(schema.kind==='tuple')return leaf(schema.items[Number(key)],rest);
 assert.equal(schema.kind,'record');const field=schema.fields.find(f=>f.name===key);assert(field,'APEX-RECORD-UNIT actual declared field '+key);return leaf(field.type,rest);
}
const cases=[['high',String.fromCharCode(0xd800)],['low',String.fromCharCode(0xdc00)],['high-tail',String.fromCharCode(0xd800)+'A'],['pair','😀'],['scalar','é'],['replacement',String.fromCharCode(0xfffd)],['literal',String.fromCharCode(92)+'ud800']];
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const rows=[];
for(const [name,location]of targets){
 const schema=plain[name]||txRecord(name);const keys=location?location.split('.'):[];
 assert.equal(leaf(schema,keys).kind,'string','APEX-RECORD-UNIT '+name+' declared String leaf');
 for(const [label,value]of cases){
  let record=sample(schema);
  if(!keys.length)record=value;
  else{let parent=record;for(const key of keys.slice(0,-1))parent=parent[key];parent[keys.at(-1)]=value;}
  const raw=JSON.stringify(record),parsed=JSON.parse(raw);
  let returned=parsed;for(const key of keys)returned=returned[key];
  if(process.env.SAFE_CEX_APEX_RECORD_OBSERVER_CONTROL===name&&label==='high')returned=returned.replaceAll(String.fromCharCode(0xd800),String.fromCharCode(0xd801));
  const observation={kind:typeof returned,units:units(returned)};
  rows.push({name,label,path:keys,raw,observation,returnedWire:JSON.stringify(returned)});
 }
}
if(process.env.SAFE_CEX_APEX_RECORD_OBSERVER_CONTROL)assert(targets.some(t=>t[0]===process.env.SAFE_CEX_APEX_RECORD_OBSERVER_CONTROL),'APEX-RECORD-UNIT named observer control');
assert.equal(rows.length,350,'APEX-RECORD-UNIT independent case floor');
console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
