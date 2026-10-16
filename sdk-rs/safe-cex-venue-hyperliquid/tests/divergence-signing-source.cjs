'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const root = path.resolve(__dirname, '../../..');
const sha = execFileSync('git', ['rev-parse', 'HEAD'], {encoding:'utf8'}).trim();
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const req = require('node:module').createRequire(path.join(process.env.SAFE_CEX_CRYPTO_ORACLE_DEPS,'package.json'));
const {ethers} = req('ethers');
const {encode} = req('@msgpack/msgpack');
assert.equal(ethers.version,'6.17.0','measured source ethers version');
const sourcePath = 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.signing.ts';
const sourceText = fs.readFileSync(path.join(root,sourcePath),'utf8');
const tree = ts.createSourceFile(sourcePath,sourceText,ts.ScriptTarget.Latest,true);
const names = ['orderTypeToWire','orderRequestToOrderWire','floatToWire','signUserSignedAction','signInner','splitSig','actionHash','addressToBytes','orderWiresToOrderAction','orderWiresToNormalTPSLAction','orderWiresToPositionalTPSLAction'];
const selected = names.map(name => {
  const nodes = tree.statements.filter(n => ts.isFunctionDeclaration(n) && n.name.text === name);
  assert.equal(nodes.length,1,'one maintained declaration '+name);
  return nodes[0].getText(tree);
}).join('\n');
const preimages = [];
const observedEthers = {...ethers,keccak256:data => {
  preimages.push(Buffer.from(data).toString('hex'));
  return ethers.keccak256(data);
}};
const context = {exports:{},ethers:observedEthers,encode,Uint8Array,DataView};
let executed = selected;
if (process.argv.includes('--asset-null-control')) {
  executed = executed.replace('a: asset,','a: null,');
  assert.notEqual(executed,selected,'asset-null control applies to actual source');
  process.stderr.write(JSON.stringify({sha,control:'asset-null'})+'\n');
}
if (process.argv.includes('--array-flatten-control')) {
  executed = executed.replace('message: action,','message: Object.assign({}, action),');
  assert.notEqual(executed,selected,'array-flatten control applies to actual source');
  process.stderr.write(JSON.stringify({sha,control:'array-flatten'})+'\n');
}
vm.runInNewContext(ts.transpileModule(executed+'\nObject.assign(exports,{'+names.join(',')+'});',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
const api = context.exports;
const bits = number => {
  const data = new DataView(new ArrayBuffer(8));
  data.setFloat64(0,number,false);
  return data.getBigUint64(0,false).toString(16).padStart(16,'0');
};
const assets = [['nan',NaN],['positive-infinity',Infinity],['negative-infinity',-Infinity],['negative-zero',-0]];
const wrappers = [['direct',undefined],['na',api.orderWiresToOrderAction],['normalTpsl',api.orderWiresToNormalTPSLAction],['positionTpsl',api.orderWiresToPositionalTPSLAction]];
const input = () => ({is_buy:true,limit_px:1,sz:2,reduce_only:false,order_type:{limit:{tif:'Gtc'}}});
(async () => {
  const prepared = [];
  for (const [assetName,asset] of assets) for (const [form,wrap] of wrappers) {
    const wire = api.orderRequestToOrderWire(input(),asset);
    assert.equal(Object.is(wire.a,asset),true,'SOURCE native asset before any wire '+assetName+'/'+form);
    assert.equal(bits(wire.a),bits(asset),'SOURCE native asset bits '+assetName+'/'+form);
    assert.equal(wire.b,true); assert.equal(wire.r,false); assert.equal(wire.p,'1'); assert.equal(wire.s,'2');
    const action = wrap ? wrap([wire]) : wire;
    const retained = wrap ? action.orders[0] : action;
    assert.equal(Object.is(retained.a,asset),true,'SOURCE native vector/group propagation '+assetName+'/'+form);
    if (wrap) { assert.equal(action.orders.length,1); assert.equal(action.orders[0],wire); assert.equal(action.grouping,form); }
    prepared.push({name:assetName+'/'+form,assetName,assetBits:bits(asset),form,asset,action});
  }
  assert.equal(prepared.length,16,'SOURCE independent four-assets by four-forms count');
  const arrayCases = [];
  const fixedFields = [{name:'0',type:'bool'},{name:'signatureChainId',type:'string'},{name:'hyperliquidChain',type:'string'}];
  const schemas = [
    ['own-fields',fixedFields],
    ['own-fields-length',[...fixedFields,{name:'length',type:'uint256'}]],
    ['own-fields-length-inherited-constructor',[...fixedFields,{name:'length',type:'uint256'},{name:'constructor',type:'bool'}]],
  ];
  const wallet = new ethers.Wallet('0x'+'11'.repeat(32));
  for (const [name,fields] of schemas) {
    const action = [false];
    let signature;
    try { signature = await api.signUserSignedAction(wallet,action,fields,'Case',true); }
    catch (error) { assert.fail('SOURCE signable native array '+name+': '+error.message); }
    assert.equal(Array.isArray(action),true,'SOURCE retains native array shape '+name);
    assert.equal(action.length,1,'SOURCE native array length '+name);
    assert.equal(action[0],false,'SOURCE native array indexed bool '+name);
    assert.equal(Object.hasOwn(action,'signatureChainId'),true);
    assert.equal(Object.hasOwn(action,'hyperliquidChain'),true);
    assert.equal(action.signatureChainId,'0x66eee'); assert.equal(action.hyperliquidChain,'Mainnet');
    assert.equal(Object.hasOwn(action,'constructor'),false,'SOURCE constructor remains inherited '+name);
    assert.equal(typeof action.constructor,'function'); assert.equal(Boolean(action.constructor),true);
    assert.equal(signature.r.length,66); assert.equal(signature.s.length,66);
    assert.equal(signature.v === 27 || signature.v === 28,true,'SOURCE native signature recovery '+name);
    arrayCases.push({name,rawAction:'[false]',payloadTypes:fields,primaryType:'Case',mainnet:true,signature:{r:signature.r,s:signature.s,v:signature.v},native:{isArray:true,length:1,indexZero:false,signatureChainId:action.signatureChainId,hyperliquidChain:action.hyperliquidChain,inheritedConstructorTruthy:true}});
  }
  assert.equal(arrayCases.length,3,'SOURCE independent three array schemas count');
  const nested = count => {let value=0; for(let i=0;i<count;i++) value=[value]; return value;};
  assert.equal(typeof api.actionHash(nested(99),null,1),'string','SOURCE MessagePack depth100 accepted');
  assert.throws(()=>api.actionHash(nested(100),null,1),error=>error.message==='Too deep objects in depth 101','SOURCE MessagePack depth101 refused');
  const marker=JSON.parse('{"$serde_json::private::Number":"120"}');
  assert.equal(typeof marker,'object','SOURCE raw codec marker native object');
  assert.deepEqual(Object.keys(marker),['$serde_json::private::Number']);
  assert.equal(marker['$serde_json::private::Number'],'120');
  const markerOrder=api.orderRequestToOrderWire({is_buy:true,limit_px:1,sz:2,reduce_only:false,order_type:{limit:{tif:'Gtc',extra:marker}}},1);
  assert.equal(markerOrder.t.limit.extra,marker,'SOURCE prepared marker retained by reference');
  assert.equal(typeof markerOrder.t.limit.extra,'object','SOURCE prepared marker native object');
  // All source native corpus assertions above precede these encoding assertions.
  assert.equal(Buffer.from(encode(marker)).toString('hex'),'81bc2473657264655f6a736f6e3a3a707269766174653a3a4e756d626572a3313230','SOURCE raw codec marker golden');
  assert.ok(JSON.stringify(markerOrder).includes('"extra":{"$serde_json::private::Number":"120"}'),'SOURCE prepared marker wire object');
  const orders = [];
  for (const row of prepared) {
    const previousCount = preimages.length;
    const hash = api.actionHash(row.action,null,1);
    assert.equal(preimages.length,previousCount+1,'one actual actionHash preimage '+row.name);
    const preimage = preimages.at(-1);
    assert.equal(typeof hash,'string'); assert.equal(hash.length,66);
    const expectedBytes = Buffer.concat([Buffer.from(encode(row.action)),Buffer.from('000000000000000100','hex')]);
    assert.equal(preimage,expectedBytes.toString('hex'),'SOURCE captured actual keccak input '+row.name);
    const wireJson = JSON.stringify(row.action);
    if (row.assetName === 'negative-zero') assert.equal((row.form === 'direct' ? JSON.parse(wireJson).a : JSON.parse(wireJson).orders[0].a),0);
    else assert.equal((row.form === 'direct' ? JSON.parse(wireJson).a : JSON.parse(wireJson).orders[0].a),null);
    orders.push({name:row.name,asset:row.assetName,assetBits:row.assetBits,form:row.form,nonce:1,vault:null,wireJson,preimage,hash});
  }
  const observations = {orders,arrays:arrayCases};
  const endSha = execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
  process.stdout.write(JSON.stringify({sha,endSha,sourcePath,sourceSha256:crypto.createHash('sha256').update(sourceText).digest('hex'),selectedProgramSha256:crypto.createHash('sha256').update(selected).digest('hex'),node:process.version,v8:process.versions.v8,arch:process.arch,ethers:ethers.version,mode:'actual-source-observation',independentCount:{assets:4,forms:4,orders:16,arrays:3,total:19},nativeAssertionsBeforeEncoding:true,observations})+'\n');
})().catch(error => {console.error(error);process.exitCode=1;});
