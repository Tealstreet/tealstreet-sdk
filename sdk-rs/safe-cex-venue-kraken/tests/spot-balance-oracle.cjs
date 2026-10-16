'use strict';
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {createRequire}=require('node:module');
const filename=path.join(__dirname,'reads-oracle.cjs');
const source=fs.readFileSync(filename,'utf8');
const prefix=source.slice(0,source.indexOf('async function account('));
if(!prefix.includes('function method('))throw Error('shared AST helper selection');
const supplement=fs.readFileSync(path.join(__dirname,'reads-spot-balance-supplement.cjs'),'utf8');
vm.runInNewContext(prefix+supplement+`
futuresFile; // Keep actual shared source selection in scope.
spotBalanceSupplement().then(corpus=>process.stdout.write(JSON.stringify({header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),arch:process.arch,normalizerSha256:crypto.createHash("sha256").update(fs.readFileSync(path.join(src,"exchanges/kraken/krakenspot.types.ts"))).digest("hex"),hashes},...corpus}))).catch(error=>{console.error(error);process.exitCode=1;});
`,{require:createRequire(filename),__dirname,Buffer,process,console},{filename});
