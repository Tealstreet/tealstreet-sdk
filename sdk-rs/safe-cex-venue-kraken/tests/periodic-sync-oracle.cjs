'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const filename=path.join(__dirname,'reads-oracle.cjs'),source=fs.readFileSync(filename,'utf8');
const cut=source.indexOf('async function account(');if(cut<0)throw Error('missing actual-source AST boundary');
const corpus=fs.readFileSync(path.join(__dirname,'periodic-sync-corpus.cjs'),'utf8');
vm.runInNewContext(source.slice(0,cut)+corpus+`
periodicCorpus().then(result=>process.stdout.write(JSON.stringify({header:{node:process.version,v8:process.versions.v8,arch:process.arch,hashes},...result}))).catch(error=>{console.error(error);process.exitCode=1});
`,{require:createRequire(filename),__dirname,Buffer,process,console},{filename});
