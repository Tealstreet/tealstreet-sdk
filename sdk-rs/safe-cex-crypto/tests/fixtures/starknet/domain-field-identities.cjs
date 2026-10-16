const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const bases=JSON.parse(fs.readFileSync(path.join(__dirname,'typed-data.json'),'utf8')).cases,cases=[];
for(const base of bases)for(const version of ['2','3']) {
 const data=JSON.parse(base.raw);data.domain.version=version;
 const raw=JSON.stringify(data),hash=typedData.getMessageHash(data,base.address);
 cases.push({revision:base.revision,version,raw,address:base.address,hash});
}
const output=JSON.stringify({label:'Self-authored Stark domain field-identity supplement; original artifacts unchanged; no independence claim',cases},null,2)+'\n',target=path.join(__dirname,'domain-field-identities.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'));
console.log(cases.length+' Stark domain identity cases');
