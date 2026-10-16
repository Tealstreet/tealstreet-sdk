const fs=require('node:fs'), path=require('node:path'), vm=require('node:vm'), assert=require('node:assert/strict');
const filename=path.join(__dirname,'utilities_oracle.cjs');
let baseline;
const localProcess={env:process.env,version:process.version,versions:process.versions,arch:process.arch,platform:process.platform,stdout:{write:text=>{baseline=JSON.parse(text);}}};
const helpers=vm.runInThisContext(`(function(require,__dirname,process){${fs.readFileSync(filename,'utf8')}\nreturn {actualMethod,market};})`,{filename})(require,__dirname,localProcess);
const supplementOutputs=[[],['first','second','third']].map(symbol=>{
 const markets=[helpers.market('BTC',{symbol})];
 const result=helpers.actualMethod('formatSymbolOut',{store:{markets}})('BTC');
 assert.equal(result.length,symbol.length,'source utility array cardinality');
 return {symbol:'BTC',markets,result};
});
process.stdout.write(JSON.stringify({...baseline,supplementOutputs}));
