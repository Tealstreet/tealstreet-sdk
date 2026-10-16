// Reuse the existing fixture host, not a second implementation of any method.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const family=process.argv[2];assert(['markets','tickers','place'].includes(family));
let code=fs.readFileSync(path.join(__dirname,'exchange_oracle.cjs'),'utf8');
function replace(before,after){assert.equal(code.split(before).length,2,`host hook: ${before.slice(0,50)}`);code=code.replace(before,after);}
replace("for(const name of ['class','constructor','markets','tickers','place','update','ohlcv','listen'])for(let i=0;i<16;i++){",`for(const name of [${JSON.stringify(family)}])for(let iteration=0;iteration<512;iteration++){const i=iteration%16,generation=Math.floor(iteration/16);`);
replace("if(Number.isNaN(v))b.writeBigUInt64BE(0x7ff8000000000000n);else b.writeDoubleBE(v);","b.writeDoubleBE(v);");
replace('let math=',`const predicateExports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'packages/math/src/isNotNumber.ts'),'utf8'),{compilerOptions:{target:99,module:1}}).outputText,{exports:predicateExports});let math=`);
replace('isNotNumber:()=>false','isNotNumber:predicateExports.isNotNumber');
replace('const fixtureEncoded=',`fixture.generation=generation;
if(name==='markets')for(const market of fixture.gamma){market.slug+='-'+generation;market.question+=' '+generation;market.orderMinSize=String(1+generation/4);}
if(name==='place'){
 if(i!==10&&i!==12)fixture.opts.price=0.1+(generation%20)/40;
 if(i!==11)fixture.opts.amount=5+generation/8;
 fixture.response.orderID='order-'+generation;
}
const fixtureEncoded=`);
replace("const n=i===0?0:i===1?1:i===2?100:i===3?101:i===4?201:3;","const n=i===0?[0,1,99,100,101,201,257,513][generation%8]:generation%4;");
replace("if(i===0&&name==='place')","if(generation===0&&i===0&&name==='place')");
// Keep the existing independent value guards for the unmodified first profile.
replace('if(!failure&&i===0){','if(!failure&&i===0&&generation===0){');
// The first placement profile keeps the old independently expected axes/ID.
replace('const fixtureEncoded=',`if(name==='place'&&generation===0){if(i<2){fixture.opts.price=0.4376;fixture.opts.amount=12.34567;}fixture.response.orderID='order-17';}const fixtureEncoded=`);
replace('cases.push({name,i,','cases.push({name,i,iteration,generation,');
replace("assert.equal(cases.length,128,'independent eight families by sixteen');assert.equal(cases[127].name,'listen');assert.equal(cases[127].i,15);","assert.equal(cases.length,512,'generated independent iteration floor');assert.equal(cases[511].iteration,511);");
new Function('require','__dirname',code)(require,__dirname);
