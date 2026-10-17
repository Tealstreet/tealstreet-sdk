const axios = require('axios');
const units = s => Array.from({length:s.length}, (_,i) => s.charCodeAt(i));
const number = n => { const b=Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
function observe(v) {
  if(v===null) return ['null'];
  if(typeof v==='number') return ['number',number(v)];
  if(typeof v==='string') return ['string',units(v)];
  if(typeof v==='boolean') return ['bool',v];
  if(Array.isArray(v)) return ['array',v.map(observe)];
  return ['object',Object.keys(v).map(k=>[units(k),observe(v[k])])];
}
const texts = [
  '"x\\ud800"','"\\udc00"','"\\ud83d\\ude00"','"\\ud800\\ud800"',
  '"x\ud800y"','{"\\ud800":"\\udfff","0":"x","__proto__":{"x":1}}',
  '{"a":["\\udfff",{"b":"x\\ud800"}],"tail":"kept"}',
  '{"\\ud800":1,"\\ud800":2,"first":3}',
  '[-0,1e400,-1e400,9007199254740993,1.234567890123456789]',
  '"\\b\\f\\n\\r\\t\\/\\\\\\\""',' \r\n\t[true,false,null,"😀"] \t',
  '01','[1,]','{"a":}','"\\uD80"','"\\x41"','"\u0001"','\ufeff{}','{} trailing'
];
const cases = texts.map(text=>{
  try{return {text:units(text),value:observe(JSON.parse(text))};}
  catch(error){return {text:units(text),error:error.name};}
});
const rest='{"label":"tp \\ud83d","\\udfff":"kept","list":[1,2],"zero":-0}';
const instance=axios.create({adapter:async config=>({data:rest,status:200,statusText:'OK',headers:{},config})});
instance.get('/source').then(response=>console.log(JSON.stringify({cases,rest,restValue:observe(response.data)})));
