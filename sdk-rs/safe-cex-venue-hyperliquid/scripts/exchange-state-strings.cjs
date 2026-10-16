const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Subject,setNow}=require('./exchange-state.cjs');
const cases=[];
for(const raw of ['"\\ud800"','"\\udfff"','"雪😀\\ufeff"','""']) {
 const id=JSON.parse(raw),s=new Subject();s.options={vault:id};setNow(0);
 s.rememberLimitOrderTimeInForce(id,'GoodTillCancel');s.suppressCancelledOrder(id);
 const result={raw,sourceAccepted:true,units:Array.from({length:id.length},(_,i)=>id.charCodeAt(i)),tif:s.getPlacedLimitOrderTimeInForce(id),cancelled:s.isOrderCancelled(id),identity:s.hasPrivateSessionIdentity,public:s.isPublicDataOnly};
 s.forgetLimitOrderTimeInForce(id);s.unsuppressCancelledOrder(id);
 result.forgotten=s.getPlacedLimitOrderTimeInForce(id)===undefined;result.unsuppressed=!s.isOrderCancelled(id);cases.push(result);
}
const output=JSON.stringify({source:'actual source state members; valid JSON code-unit strings before serialization',cases},null,2)+'\n';
const target=path.join(__dirname,'../tests/fixtures/exchange-state-strings.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'),'exchange state string witnesses drift');
