const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('ethers');
const ethers=oracleRequire('ethers'),body='Z'.repeat(31);
const expanded=(body+'XE00').split('').map(char=>/[0-9]/.test(char)?char:String(char.charCodeAt(0)-55)).join('');
const input='XE'+String(98n-BigInt(expanded)%97n).padStart(2,'0')+body,address=ethers.getAddress(input);
assert.ok(address.length>42,'SDK oversized direct ICAP acceptance must be measured');
const result=JSON.stringify({oracle:'pinned ethers oversized direct-mode ICAP behavior',input,address,isAddress:ethers.isAddress(input)},null,2)+'\n',target=path.join(__dirname,'icap-maximum.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'ethers oversized ICAP drift');
