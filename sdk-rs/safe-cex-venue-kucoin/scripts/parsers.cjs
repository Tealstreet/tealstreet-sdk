const fs=require('node:fs');
const {context}=require('./order-helpers.cjs');
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(cases.map(({name,args})=>{
 try{return {ok:context[name](...args)};}catch(error){return {error:String(error)};}
})));
