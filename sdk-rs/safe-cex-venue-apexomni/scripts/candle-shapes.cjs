// Reuse the source loader and actual shared DefaultStore; replace only its driver.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'lifecycle.cjs');
let code=require('./controlled-exchange-loader.cjs')(fs.readFileSync(filename,'utf8'));
const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,'APEX-CANDLE maintained driver selector');
code=code.replace(driver,()=>String.raw`
(async()=>{
 const candle={t:2000,o:'10.25',h:'12.25',l:'9.25',c:'11.25',v:'7.25'};
 const cases=[
  ['array-valid',[candle,{...candle,t:1000,v:'8.25'}]],['array-empty',[]],
  ['array-null',[candle,null]],
  ['array-mixed-nan',[{...candle,t:'bad',v:'1.25'},{...candle,t:1000,v:'2.25'},{...candle,t:'bad',v:'3.25'},{...candle,t:2000,v:'4.25'}]],
  ['array-t-null',[{...candle,t:null}]],['array-scalar',[17]],
  ['array-bad-t',[{...candle,t:{toString:null}}]],
  ['object-map-absent',{}],['object-map-null',{map:null}],['object-map-false',{map:false}],
  ['object-map-number',{map:17}],['object-map-string',{map:'map'}],['object-map-array',{map:[]}],
  ['truthy-number',17],['truthy-boolean',true],['truthy-string','x'],
  ['falsy-null',null],['falsy-zero',0],['falsy-boolean',false],
  ['missing-symbol',undefined],['null-data',undefined],['null-root',undefined],['missing-data',undefined],
  ['object-map-undefined',{map:undefined}],
 ];
 const rows=[];
 for(const [label,value] of cases){
  const venue=new Class('apexomni',{key:'key',secret:'secret'},store());
  const payload=label==='missing-symbol'?{data:{}}:label==='null-data'?{data:null}:label==='null-root'?null:
                label==='missing-data'?{}:{data:{BTCUSDC:value}};
  responses={'/v3/klines':payload};operations=[];
  const opts={symbol:'BTCUSDC',interval:'1m',limit:3,to:1700001234567};
  let result;
  try{result={ok:true,value:native(await venue.fetchOHLCV(opts))};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({label,payload:native(payload),result,operations:native(operations)});
 }
 assert.equal(rows.length,24,'APEX-CANDLE independent source floor');
 console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
