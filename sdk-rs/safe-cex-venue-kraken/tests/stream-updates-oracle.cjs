'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const filename=path.join(__dirname,'reads-oracle.cjs'),source=fs.readFileSync(filename,'utf8');
const cut=source.indexOf('async function account(');if(cut<0)throw Error('missing actual-source AST boundary');
const corpus=fs.readFileSync(path.join(__dirname,'stream-updates-corpus.cjs'),'utf8');
vm.runInNewContext(source.slice(0,cut)+corpus+`
const result=process.argv.includes('--date-inputs')
 ? {dates:[[2020],['2020-01-02T03:04:05.678Z'],[],[null],[2020,1,2],{},null,true,1000.75,'2020-01-02T03:04:05.678Z','bad date'].map(input=>({input,...streamCase('Candle',{fields:{interval_begin:{kind:'json',text:JSON.stringify(input)}}})}))}
 : streamCorpus();process.stdout.write(JSON.stringify({header:{node:process.version,v8:process.versions.v8,arch:process.arch,tz:process.env.TZ,hashes},...result}));
`,{require:createRequire(filename),__dirname,Buffer,process,console},{filename});
