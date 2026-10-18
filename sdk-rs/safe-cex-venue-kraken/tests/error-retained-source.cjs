// Reuse the maintained AST selector; execute the unchanged live TS constructor.
'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const filename=path.join(__dirname,'helpers-oracle.cjs');
const text=fs.readFileSync(filename,'utf8'),cut=text.indexOf('(async()=>{');
if(cut<0)throw Error('maintained AST helper boundary missing');
vm.runInNewContext(text.slice(0,cut)+`
const api=declarations(path.join(src,'exchanges/kraken/kraken.api.ts'),['createKrakenApiError'],{});
const response={fraction:-0};
const error=api.functions.createKrakenApiError('literal {broken json}',response);
assert.equal(error.response,response); assert.ok(Object.is(error.response.fraction,-0));
const projected=JSON.parse(JSON.stringify(error.response));
console.log(JSON.stringify({sourceNativeBits:capture(error.response.fraction).number,
 sourceIdentity:error.response===response,projectedBits:capture(projected.fraction).number}));
`,{require:createRequire(filename),__dirname,process,Buffer,console},{filename});
