// Test-only fault injection AFTER the unchanged source oracle has completed.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const filename=path.join(__dirname,process.env.PHEMEX_ORACLE_FILE);
const localProcess={env:process.env,version:process.version,versions:process.versions,arch:process.arch,platform:process.platform,exitCode:0,stdout:{write:text=>{
 const corpus=JSON.parse(text);
 const target=process.env.PHEMEX_DROP_PATH.split('.').reduce((value,key)=>value[key],corpus);
 const keep=process.env.PHEMEX_DROP_MODE==='empty'?0:1;
 assert(target && typeof target==='object','source control path exists');
 if(Array.isArray(target))target.splice(keep);
 else Object.keys(target).slice(keep).forEach(key=>delete target[key]);
 process.stdout.write(JSON.stringify(corpus));
}}};
vm.runInThisContext(`(function(require,__dirname,process){${fs.readFileSync(filename,'utf8')}\n})`,{filename})(require,__dirname,localProcess);
