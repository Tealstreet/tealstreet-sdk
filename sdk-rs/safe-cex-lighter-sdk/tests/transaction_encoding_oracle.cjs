const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function extract(relative,names){const file=path.join(root,relative),src=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);return names.map(name=>{const hits=src.statements.filter(n=>n.name?.text===name);assert.equal(hits.length,1,name+' maintained source');return hits[0].getText(src);}).join('\n');}
const hashNames=['hashCreateOrderTx','hashCancelOrderTx','hashTransferTx','hashWithdrawTx','hashAuthToken'],amountNames=['bigIntToHex','formatAmount','parseAmount'];
const declarations=extract('packages/lighter-sdk/src/signer/field-elements.ts',['uint32ToFieldElement','uint64ToFieldElement','int64ToFieldElement','concatenateFieldElements'])+'\n'+extract('packages/lighter-sdk/src/signer/transaction-hasher.ts',hashNames)+'\n'+extract('packages/lighter-sdk/src/utils/index.ts',amountNames);
const compiled=ts.transpileModule(declarations+'\n({'+[...hashNames,...amountNames].join(',')+'})',{fileName:'maintained-encoding.ts',reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));
const source=vm.runInNewContext(compiled.outputText,{exports:{}});
function units(s){return Array.from({length:s.length},(_,i)=>s.charCodeAt(i));}
function pack(v){if(typeof v==='string')return{units:units(v)};if(typeof v==='bigint')return{bigint:String(v)};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:Number.isNaN(v)?'NaN':b.toString('hex')}};if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,pack(v)]));return v;}
function run(method,args,label){let output;try{output={ok:pack(source[method](...args))};}catch(e){output={error:{name:e.name,message:units(e.message)}}}return{method,label,args:pack(args),output};}
const cases=[];
for(const [kind,method]of hashNames.entries())for(let v=0;v<8;v++){
 let chain=19;
 const tx={txType:[14,15,16,17][kind]??14,opts:{accountIndex:3,apiKeyIndex:5,nonce:7,expiry:11},marketId:'0xAbc',orderId:'0x0123456789abcdefABC',orderType:2,side:1,price:'-123456789',quantity:'0x123',timeInForce:2,reduceOnly:true,postOnly:false,tokenAddress:'0xF00',recipient:'0x123456789abcdef01',amount:'-54321',fastWithdraw:true};
 if(v===1){tx.marketId='';tx.orderId='0x';tx.tokenAddress='0X00';tx.recipient='';tx.stopPrice='0';tx.l1Recipient='0x12';tx.fastWithdraw=false;}
 if(v===2){chain=NaN;tx.opts.accountIndex=-0;tx.opts.apiKeyIndex=5.75;tx.opts.nonce=NaN;tx.opts.expiry=0xffffffff;tx.orderType=NaN;tx.side=0.5;tx.marketId='0x'+'Z'.repeat(16)+'\ud800';tx.l1Recipient='';}
 if(v===3){chain=-1;tx.price='bad';tx.amount='bad';}
 if(v===4){tx.opts.accountIndex=Infinity;}
 if(v===5){tx.marketId='\ud800';tx.orderId='0Xabcdef';tx.price='oops\udfff';tx.amount='oops\ud800';}
 if(v===6){tx.price='-9223372036854775808';tx.quantity='  +12  ';tx.amount='-9223372036854775808';tx.stopPrice='';tx.l1Recipient='abc'+ 'D'.repeat(16);}
 if(v===7){tx.quantity='9223372036854775808';tx.amount='9223372036854775808';tx.marketId='0xabcxyz';tx.l1Recipient=undefined;tx.fastWithdraw=undefined;}
 const authArgs=[[3,5,123456789012345n,19],[0,255,1n,0],[NaN,-0,0n,NaN],[-1,5,-1n,19],[3,Infinity,5n,19],[3,5,-1n,19],[0xffffffff,1.5,18446744073709551615n,0xdeadbeef],[3,5,18446744073709551616n,19]];
 const args=kind===4?authArgs[v]:[tx,chain];cases.push(run(method,args,v));
}
const utilityArgs={
 bigIntToHex:[[255n],[255n,4],[-255n,4],[0n,-0],[2n**300n,1],[255n,3.75],[12n,NaN],[255n,-2],[1n,Infinity],[1n,0.5],[1n,0],[1n,-Infinity]],
 formatAmount:[[123456789n,8],[100000000n,8],[-1230n,3],[-5n,3],[2n**300n,0],[1n,-0],[1n,-1],[1n,0.5],[1n,NaN],[1n,Infinity],[1n,-Infinity],[0n,2]],
 parseAmount:[['1.234',3],['0001.2',3],['12.',0],['12',-0],['999999999999999999999999999999999',2],['1.23',2.5],['1.2345',2],['-1',2],['1\ud800',2],['12.34',NaN],['1\n',0],['1',-1],['1',Infinity],['',2],['.1',1],['1.2.3',2]]
};
for(const method of amountNames)utilityArgs[method].forEach((args,i)=>cases.push(run(method,args,i)));
process.stdout.write(JSON.stringify({runtime:{node:process.version,platform:process.platform,arch:process.arch},cases}));
