const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const directory = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid');
const cache = new Map();
const math = Object.create(Math);
const walletEnvironment = {};
const agentContext = {exports:{},Math:math,__walletEnvironment:walletEnvironment};
function load(name) {
  if(cache.has(name)) return cache.get(name);
  const source = fs.readFileSync(path.join(directory,name),'utf8');
  const context = name === 'hyperliquid.agent.ts' ? agentContext : {exports:{}};
  context.require = id => {
    if(id === 'ethers') return {ethers:new Proxy({}, {get() {throw new Error('unexpected ethers access in leaf oracle');}})};
    if(id === '../../telemetry') return {reportEvent() {throw new Error('unexpected telemetry call in leaf oracle');}};
    const local = {'./hyperliquid.constants':'hyperliquid.constants.ts','./hyperliquid.types':'hyperliquid.types.ts','./hyperliquid.builderFee':'hyperliquid.builderFee.ts'};
    assert(local[id],`unexpected source import ${id}`);
    return load(local[id]);
  };
  const compiled = ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
  // Ordinary lexical object-environment access avoids Node VM global accessor
  // interception, which can replace a thrown getter value with ReferenceError.
  vm.runInNewContext(name==='hyperliquid.agent.ts' ? `(function(){with(__walletEnvironment){${compiled}}})();` : compiled,context);
  cache.set(name,context.exports);
  return context.exports;
}
const builder = load('hyperliquid.builderFee.ts'), agent = load('hyperliquid.agent.ts');
const constantNames=['DEFAULT_BUILDER_FEE_TENTHS_BPS','DEFAULT_BUILDER_FEE_RATE','HYPERLIQUID_BUILDER_DOMAIN','HYPERLIQUID_BUILDER_DOMAIN_TESTNET','APPROVE_BUILDER_FEE_TYPES'];
const constants=Object.fromEntries(constantNames.map(name=>[name,builder[name]]));
constants.APPROVE_AGENT_TYPES=agent.APPROVE_AGENT_TYPES;
const bits = number => {const buffer=Buffer.alloc(8);buffer.writeDoubleBE(Number.isNaN(number)?NaN:number);return buffer.toString('hex');};
const names=[];
function nameCase(input, errorMode=null, errorAt=null) {
  let slot=0; const reads=[]; const sentinel={sentinel:true};
  Object.defineProperty(math,'random',{configurable:true,get() {
    reads.push(`get:${slot}`);
    if(errorMode==='get'&&slot===errorAt) throw sentinel;
    return () => {
      reads.push(`call:${slot}`);
      if(errorMode==='call'&&slot===errorAt) throw sentinel;
      return input[slot++];
    };
  }});
  const row={inputBits:input.map(bits),errorMode,errorAt,reads};
  try {row.value=agent.generateAgentWalletName();}
  catch(error) {assert.equal(error,sentinel,'source random error identity');row.errorIsSentinel=error===sentinel;}
  names.push(row);
}
for(let index=0;index<36;index++) nameCase(Array(4).fill((index+.25)/36));
for(const input of [
  [0,.5,26/36,1-Number.EPSILON], [-0,-Number.MIN_VALUE,1,Infinity],
  [NaN,-Infinity,1.25,-.5], [1/36-Number.EPSILON,1/36,1/36+Number.EPSILON,Number.MIN_VALUE],
  [.1,.2,.3,.4], [NaN,NaN,NaN,NaN],
]) nameCase(input);
for(const mode of ['get','call']) for(let index=0;index<4;index++) nameCase([0,.25,.5,.75],mode,index);
assert.equal(names.length,50,'independent name corpus floor');
const wallets=[];
const walletInputs=[['missing'],['undefined'],['null'],['false'],['true'],['zero'],['negativeZero'],['nan'],['empty'],['loneSurrogate'],['object'],['array'],['zeroBigInt'],['nonzeroBigInt'],['symbol'],['windowError'],['secondWindowError'],['ethereumError']];
for(const [mode] of walletInputs) {
  delete walletEnvironment.window;
  const reads=[],sentinel={sentinel:true}; let windowReads=0;
  const values={undefined:undefined,null:null,false:false,true:true,zero:0,negativeZero:-0,nan:NaN,empty:'',loneSurrogate:'\ud800',object:{},array:[],zeroBigInt:0n,nonzeroBigInt:1n,symbol:Symbol('x')};
  if(mode!=='missing') Object.defineProperty(walletEnvironment,'window',{configurable:true,get() {
    reads.push('window'); windowReads++; if(mode==='windowError'||(mode==='secondWindowError'&&windowReads===2)) throw sentinel;
    return {get ethereum() {reads.push('ethereum');if(mode==='ethereumError')throw sentinel;return values[mode];}};
  }});
  const row={mode,reads};
  try {row.value=agent.hasEthereumWallet();}
  catch(error) {assert.equal(error,sentinel,'source wallet error identity');row.errorIsSentinel=error===sentinel;}
  wallets.push(row);
}
assert.equal(wallets.length,18,'independent wallet corpus floor');

// The closed spot cancellation root, inspected independently of core bodies.
const spotPath=path.join(directory,'hyperliquidspot.types.ts');
const parse=(file)=>ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const spot=parse(spotPath), core=parse(path.join(directory,'hyperliquid.types.ts'));
const printer=ts.createPrinter({removeComments:true});
const declaration=(file,name)=>file.statements.find(node => (ts.isTypeAliasDeclaration(node)||ts.isInterfaceDeclaration(node)) && node.name.text===name);
assert.equal(printer.printNode(ts.EmitHint.Unspecified,declaration(spot,'CancelOrderRequests').type,spot),printer.printNode(ts.EmitHint.Unspecified,declaration(core,'CancelOrderRequests').type,core),'closed cancellation list source declaration');
const cancellations=[];
function add(type,inputs,acceptedCount) {inputs.forEach((input,index)=>cancellations.push({type,input,expectedAccepted:index<acceptedCount}));}
add('CancelOrderRequests',[
  '[]','[{"a":17.5,"o":91.25},{"a":-0,"o":1e400}]','[{"a":-1e400,"o":9007199254740993}]',
  '{}','[{}]','[{"a":null,"o":1}]','[{"a":1,"o":"2"}]','[[1,2]]',
],3);
assert.equal(cancellations.length,8,'independent cancellation case floor');
const probePath=path.join(directory,'__approval_leaves_probe__.ts');
const probe="import * as Spot from './hyperliquidspot.types';\n"+cancellations.map((row,index)=>`const case_${index}: Spot.${row.type} = ${row.input};`).join('\n');
const options={strict:true,noEmit:true,target:ts.ScriptTarget.ES2022,skipLibCheck:true};
const host=ts.createCompilerHost(options),read=host.readFile.bind(host),exists=host.fileExists.bind(host);
host.readFile=file=>file===probePath?probe:read(file); host.fileExists=file=>file===probePath||exists(file);
const program=ts.createProgram([probePath],options,host), file=program.getSourceFile(probePath);
assert(file,'cancellation compiler input');
assert.equal(program.getSyntacticDiagnostics(file).length,0,'cancellation probe syntax');
const rejected=new Set();
for(const diagnostic of program.getSemanticDiagnostics(file)) {
  assert(diagnostic.start!==undefined);
  const line=file.getLineAndCharacterOfPosition(diagnostic.start).line;
  assert(line>0&&line<=8);
  assert([2322,2740,2741,2739,2559,2353].includes(diagnostic.code),`unexpected compiler diagnostic ${diagnostic.code}`);
  rejected.add(line-1);
}
for(const [index,row] of cancellations.entries()) {
  row.accepted=!rejected.has(index);
  assert.equal(row.accepted,row.expectedAccepted,`actual cancellation checker ${row.type} case ${index}`);
  if(!row.accepted) continue;
  const value=JSON.parse(row.input);
  row.native=row.type==='CancelOrderRequests'?value.map(item=>({aBits:bits(item.a),oBits:bits(item.o)})):{type:value.type,timeKind:!Object.hasOwn(value,'time')?'absent':value.time===null?'null':'number',timeBits:typeof value.time==='number'?bits(value.time):null};
}
console.log(JSON.stringify({constants,names,wallets,cancellations,iterationStop:{names:50,wallets:18,cancellations:8}}));
