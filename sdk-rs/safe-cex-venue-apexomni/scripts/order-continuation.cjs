// Observe the maintained mapper before typed Rust output admission.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'exchange.cjs');
let code=require('./controlled-exchange-loader.cjs')(fs.readFileSync(filename,'utf8'));
const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,'APEX-ORDER maintained driver selector');
code=code.replace(driver,()=>String.raw`
(async()=>{
 const none={openSlParams:{triggerPrice:0},openTpParam:{triggerPrice:0}};
 const bad={toString:null};
 const cases=[
 ['valid',{}],['type-function',{...none,type:'constructor'}],['type-prototype',{...none,type:'__proto__'}],
 ['type-missing',{...none,type:'UNKNOWN'}],['status-function',{...none,status:'constructor'}],['status-prototype',{...none,status:'__proto__'}],
 ['type-before-price',{...none,type:'constructor',price:bad}],['type-clock',{...none,type:'constructor'}],
 ['tp-before-sl-clock',{openTpParam:{triggerPrice:bad}}],
 ['missing-market',{symbol:'MISSING',price:bad}],['missing-market-bad-type',{symbol:'MISSING',type:bad}],
 ['numeric-side',{side:17}],['null-side',{side:null}],['numeric-id',{id:17}],['numeric-reduce',{reduceOnly:17}],
 ['array-type',{type:['LIMIT']}],['array-status',{status:['OPEN']}],
 ['bad-type-key',{type:bad}],['bad-tif-key',{timeInForce:bad}],['bad-status-key',{status:bad}],
 ['bad-base-trigger-key',{...none,type:'STOP_MARKET',triggerPriceType:bad}],
 ['bad-tp-trigger-key',{openTpParam:{triggerPrice:'122.25',triggerPriceType:bad}}],
 ['bad-id-interpolation',{id:bad}],
 ['optional-tif-function',{...none,timeInForce:'constructor'}],['optional-tif-prototype',{...none,timeInForce:'__proto__'}],
 ['optional-trigger-function',{...none,type:'STOP_MARKET',triggerPriceType:'constructor'}],
 ['optional-trigger-prototype',{...none,type:'STOP_MARKET',triggerPriceType:'__proto__'}],
 ['reduce-before-price',{reduceOnly:17,price:bad}],
 ];
 const rows=[];
 for(const [label,patch] of cases){
  const input={...order,...patch};const venue=new Class('apexomni',{key:'key',secret:'secret'},store());operations=[];
  let clocks=0;context.virtualClock.getCurrentTimeMs=()=>{clocks++;return clock};
  let result;
  try{const value=venue.mapOrder(input);result={ok:true,value:native(value),wire:JSON.stringify(value)};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({label,input:native(input),result,clocks,operations:native(operations)});
 }
 assert.equal(rows.length,28,'APEX-ORDER independent source floor');
 console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
