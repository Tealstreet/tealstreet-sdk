const keys = new Set(['', '0', '-0', '+0', '00', '01', '1.0', '1e0', ' 1', '1 ', '-1', '+1', '4294967294', '4294967295', '4294967296', '9007199254740991', '\ufeff0', '\u00850', '😀', 'items', 'constructor', '__proto__']);
for(let i=0;i<1024;i++) for(const key of [String(i), '0'+i, '+'+i, i+'.0', i+'e0']) keys.add(key);
const array=[{items:[['nested','pair']]}, null, false, 0, '', {items:[]}];
const object=Object.fromEntries([...keys].map(key=>[key,'value:'+key]));
const rows=[...keys].map(key=>{
 const probe=[];Object.defineProperty(probe,key,{value:true,configurable:true});
 const index=probe.length ? probe.length-1 : null;
 const own=(target)=>Object.hasOwn(target,key)?{found:true,value:target[key]}:{found:false};
 return {key,index,array:own(array),object:own(object)};
});
process.stdout.write(JSON.stringify({rows,array,object,nested:array['0']['items']}));
