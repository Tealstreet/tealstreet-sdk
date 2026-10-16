import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const source=ts.createSourceFile('bybit.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const owner=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const members=['apiKeyInfo','cachedApiKeyInfo'].map(name=>{
  const selected=owner.members.filter(n=>n.name?.getText(source)===name);
  assert.equal(selected.length,1,`KEY-CACHE source member ${name}`);
  return selected[0].getText(source);
});
const create=()=>vm.runInNewContext(ts.transpileModule(`new class { ${members.join('\n')} }`,{}).outputText);
const bits=n=>{
  if(Number.isNaN(n))return '7ff8000000000000';
  const bytes=Buffer.alloc(8);bytes.writeDoubleBE(n);return bytes.toString('hex');
};
const record=()=>({id:'id-initial',note:'note-initial',apiKey:'api-initial',secret:'secret-initial',
  readOnly:0,permissions:{ContractTrade:['trade-initial'],Spot:['spot-initial'],Wallet:['wallet-initial'],Options:['option-initial'],Derivatives:['derivative-initial'],CopyTrading:['copy-initial'],BlockTrade:['block-initial'],Exchange:['exchange-initial'],NFT:['nft-initial']},ips:['ip-initial'],type:2,deadlineDay:3,expiredAt:'expired-initial',createdAt:'created-initial',unified:4,uta:5,userID:6,inviterID:7,vipLevel:'vip-initial',mktMakerLevel:'maker-initial',affiliateID:8,rsaPublicKey:'rsa-initial',isMaster:true,parentUid:'parent-initial'});
const cases=[];
for(const number of [0,-0,1.25,-2.5,Number.MIN_VALUE,NaN,Infinity,-Infinity]){
  const subject=create();assert.equal(subject.cachedApiKeyInfo,null,'KEY-CACHE initial null');
  const input=record();subject.apiKeyInfo={...input,readOnly:number};
  const view=subject.cachedApiKeyInfo;assert.equal(view,subject.apiKeyInfo,'KEY-CACHE source alias');
  subject.apiKeyInfo.note='owner-note';subject.apiKeyInfo.permissions.ContractTrade.push('owner-trade');subject.apiKeyInfo.ips.push('owner-ip');
  const afterOwner={note:view.note,trade:[...view.permissions.ContractTrade],ips:[...view.ips],number:bits(view.readOnly)};
  view.id='consumer-id';view.permissions.Spot.push('consumer-spot');view.ips.push('consumer-ip');
  const afterConsumer={id:subject.apiKeyInfo.id,spot:[...subject.apiKeyInfo.permissions.Spot],ips:[...subject.apiKeyInfo.ips]};
  subject.apiKeyInfo=record();subject.apiKeyInfo.id='replacement-id';
  const replacement={oldId:view.id,freshId:subject.cachedApiKeyInfo.id};
  subject.apiKeyInfo=null;assert.equal(subject.cachedApiKeyInfo,null,'KEY-CACHE cleared null');
  cases.push({input:record(),number:bits(number),afterOwner,afterConsumer,replacement,oldAfterClear:view.id});
}
assert.equal(cases.length,8,'KEY-CACHE source total');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.versions.node,nan:'canonical NaN only; other bits retained'},cases}));
