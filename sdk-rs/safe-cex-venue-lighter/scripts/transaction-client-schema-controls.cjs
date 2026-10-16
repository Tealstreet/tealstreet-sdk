'use strict';
// In-memory source substitutions in isolated children. No production file edits.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),{spawnSync}=require('node:child_process');
const oracle=path.join(__dirname,'transaction-client-oracle.cjs'),http=path.resolve(__dirname,'../../../packages/lighter-sdk/src/client/http-types.ts');
const original=fs.readFileSync(http,'utf8'),digest='794a306617545328da0c72bd9fb122663849f4526ed3aac6a0e3355f9751ee68';
function capture(){const r=spawnSync(process.execPath,[oracle],{encoding:'utf8'});assert.equal(r.status,0,r.stderr);assert.equal(JSON.parse(r.stdout).contentHash,digest,'restored source corpus');return r.status;}
const baseline=capture();
function replaceOnce(from,to){assert.equal(original.split(from).length-1,1,'unique schema control sentinel');return original.replace(from,to);}
const controls=[
 ['inherited-required-field',replaceOnce('export interface SendTxBatchResponse {','export interface SendTxBatchResponse extends ReviewExtra {')+'\ninterface ReviewExtra { hiddenRequired: number; }\n','batch response no inherited schema'],
 ['hash-method-instead-of-property',replaceOnce('txHashes: string[];','txHashes(): string[];'),'batch response property fields'],
];
const results=[];
for(const [name,source,assertion]of controls){
 const hook="const fs=require('node:fs');const i=JSON.parse(fs.readFileSync(0,'utf8'));const read=fs.readFileSync;fs.readFileSync=function(p,...args){return String(p)===i.http?i.source:read.call(this,p,...args)};require(i.oracle);";
 const r=spawnSync(process.execPath,['-e',hook],{encoding:'utf8',input:JSON.stringify({http,source,oracle})});
 assert.equal(r.status,1,`${name} must fail`);assert.ok(r.stderr.includes(assertion),`${name} named assertion`);results.push({name,exit:r.status,assertion});
}
const restored=capture();
const gateSha=spawnSync('git',['rev-parse','HEAD'],{encoding:'utf8'});assert.equal(gateSha.status,0);
process.stdout.write(JSON.stringify({gateSha:gateSha.stdout.trim(),baseline,controls:results,restored})+'\n');
