const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'exchange.cjs');
let code=require('./controlled-exchange-loader.cjs')(fs.readFileSync(filename,'utf8'));
const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,"APEX-CONT maintained driver selector");
code=code.replace(driver,()=>String.raw`
(async()=>{
 const rows=[];
 for(const [name,patch] of [
 ['mapPosition:ignored-custom',{customImr:'0.2',customInitialMarginRate:{toString:null}}],
 ['mapPosition:ignored-default',{customImr:'0.2'}],
 ['mapPosition:missing-market-unrealised',{symbol:'MISSING',unrealisedPnl:{toString:null}}],
 ['mapOrder:inherited-type',{type:'constructor',price:{toString:null},triggerPrice:0}],
 ['mapOrder:bad-type-clock',{type:'constructor',openSlParams:{triggerPrice:0},openTpParam:{triggerPrice:0}}],
 ['mapOrder:bracket-order',{openSlParams:{triggerPrice:'88.25'},openTpParam:{triggerPrice:{toString:null}}}],
 ['fetchMarketsHelper:filter-before-map',{}],
 ]){
  const s=store(); operations=[];
  if(name==='mapPosition:ignored-default')s.markets[0].info.defaultInitialMarginRate={toString:null};
  const venue=new Class('apexomni',{key:'key',secret:'secret'},s);operations=[];
  let clocks=0;context.virtualClock.getCurrentTimeMs=()=>{clocks++;return clock};
  if(name==='fetchMarketsHelper:filter-before-map') responses['/v3/symbols']={data:{contractConfig:{perpetualContract:[{...contract,enableTrade:'truthy'},null]}}};
  try{
   const result=name.startsWith('mapPosition')?venue.mapPosition({...rawPosition,...patch}):name.startsWith('mapOrder')?venue.mapOrder({...order,...patch}):await venue.fetchMarketsHelper();
   rows.push({name,input:name.startsWith('mapPosition')?{...rawPosition,...patch}:undefined,market:s.markets[0],ok:true,result:native(result),clocks,operations:native(operations)});
  }catch(error){rows.push({name,input:name.startsWith('mapPosition')?{...rawPosition,...patch}:undefined,market:s.markets[0],ok:false,error:{name:error.name,message:error.message},clocks,operations:native(operations)})}
 }
 assert.equal(rows.length,7,"APEX-CONT independent source case count");
 console.log(JSON.stringify({header:{node:process.version,arch:process.arch},rows}));
})().catch(error=>{console.error(error);process.exitCode=1});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
