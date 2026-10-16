'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const filename=path.join(__dirname,'reads-oracle.cjs'),source=fs.readFileSync(filename,'utf8');
const cut=source.indexOf('async function account(');if(cut<0)throw Error('missing actual-source AST boundary');
const corpus=fs.readFileSync(path.join(__dirname,'stream-updates-corpus.cjs'),'utf8');
vm.runInNewContext(source.slice(0,cut)+corpus+`
const result=streamCorpus();process.stdout.write(JSON.stringify({header:{node:process.version,v8:process.versions.v8,arch:process.arch,tz:process.env.TZ,hashes},...result}));
`,{require:createRequire(filename),__dirname,Buffer,process,console},{filename});
