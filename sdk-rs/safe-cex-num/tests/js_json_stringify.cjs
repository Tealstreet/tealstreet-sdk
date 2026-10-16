const u=['undefined'], n=x=>['number',String(x)], s=x=>['string',x], a=x=>['array',x], o=x=>['object',x];
function value(v) { switch(v[0]) {
  case 'undefined':return undefined; case 'number':return Number(v[1]); case 'string':return v[1];
  case 'array':return v[1].map(value); case 'object':return Object.fromEntries(v[1].map(([k,v])=>[k,value(v)]));
  case 'bigint':return BigInt(v[1]); case 'json':return JSON.parse(v[1]);
}}
const inputs=[u,n('-0'),n('NaN'),n('Infinity'),n('-Infinity'),['bigint','10'],s('\ufeff'),s('\u0000\n\t"\\\u2028😀'),['json','{"z":1e400,"2":-0,"1":9007199254740993}']];
for(let e=-324;e<=308;e++) {
  const v=n('1e'+e);
  inputs.push(v,a([u,v]),o([['z',v],['10',v],['2',u],['01',s('\ufeff')],['a',a([u,o([['b',u],['a',v]])])]]));
}
inputs.push(o([['b',n(1)],['a',n(2)],['b',u]]),o([['b',u],['a',n(2)],['b',n(3)]]));
for(const v of [['bigint','10'],u]) inputs.push(a([v]),o([['a',v]]));
process.stdout.write(JSON.stringify(inputs.map(input=>{
  try {const text=JSON.stringify(value(input));return {input,expected:{ok:text===undefined?null:text}};}
  catch(e) {return {input,expected:{error:{name:e.name,message:e.message}}};}
})));
