const keys=['b','a','10','2','0','01','1.0','-1','-0','+1','4294967294','4294967295','4294967296','1e0','','\ufeff','\ufeff1','é','__proto__'];
const rows=[];
for(let i=0;i<300;i++) {
  const ks=keys.slice(i%keys.length).concat(keys.slice(0,i%keys.length));
  ks.push(String(i), String(4294967000+i), String(i)+'.0');
  const entries=[...new Set(ks)].map((k,j)=>[k,{z:j,'12':12,'2':2,'01':1}]);
  const o=Object.fromEntries(entries);
  rows.push({keys:entries.map(([k])=>k),expected:Reflect.ownKeys(o),raw:'{'+entries.map(([k,v])=>JSON.stringify(k)+':'+JSON.stringify(v)).join(',')+'}',json:JSON.stringify(o)});
}
process.stdout.write(JSON.stringify(rows));
