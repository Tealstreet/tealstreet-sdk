const assert=require('node:assert/strict');
const http=require('node:http');
const axios=require('axios').default;
const cases=[
[{a:[1,2]},'a%5B%5D=1&a%5B%5D=2'],
[{a:'f[g]h'},'a=f%5Bg%5Dh'],
[{a:"x~y!z(1)'"},"a=x~y!z(1)'"],
[{a:{b:1}},'a%5Bb%5D=1'],
[{a:[{b:1}]},'a%5B0%5D%5Bb%5D=1'],
[{'tags[]':['a b',null,'x+y']},'tags%5B%5D=a+b&tags%5B%5D=x%2By'],
[{p:NaN,i:Infinity,n:-Infinity,z:-0,skip:undefined,nil:null},'p=NaN&i=Infinity&n=-Infinity&z=0'],
[{a:{b:NaN,c:[Infinity,-0]}},'a%5Bb%5D=NaN&a%5Bc%5D%5B0%5D=Infinity&a%5Bc%5D%5B1%5D=0'],
[{'meta{}':{n:NaN,i:Infinity,z:-0}},'meta%7B%7D=%7B%22n%22:null,%22i%22:null,%22z%22:0%7D'],
[{'list[]':{0:{a:1},2:NaN,length:3}},'list%5B%5D=%5Bobject+Object%5D&list%5B%5D=NaN'],
[{' x ':1,' nest ':{' y ':2}},'x=1&+nest+%5By%5D=2'],
[{10:1,2:2,a:3},'2=2&10=1&a=3'],
[{a:[[],[null,undefined]],b:[]},''],
[{a:':$, + /?&=ñ'},'a=:$,+%2B+%2F%3F%26%3D%C3%B1'],
[{a:[true,false,3n]},'a%5B%5D=true&a%5B%5D=false&a%5B%5D=3'],
];
for(const [params,expected] of cases)assert.equal(axios.getUri({url:'/witness',params}),'/witness'+(expected?'?'+expected:''));
const nested=count=>Array.from({length:count}).reduce(value=>({next:value}),true);
assert.equal(axios.getUri({url:'/witness',params:{'payload{}':nested(100)}}),'/witness?payload%7B%7D='+'%7B%22next%22:'.repeat(100)+'true'+'%7D'.repeat(100));
assert.throws(()=>axios.getUri({url:'/witness',params:{'payload{}':nested(101)}}),{message:'Object is too deeply nested (101 levels). Max depth: 100'});
assert.equal(axios.getUri({url:'/witness',params:nested(101)}),'/witness?next'+'%5Bnext%5D'.repeat(100)+'=true');
assert.throws(()=>axios.getUri({url:'/witness',params:nested(102)}),{message:'Object is too deeply nested (101 levels). Max depth: 100'});
(async()=> {
 const received=[];
 const server=http.createServer((req,res)=>{received.push(req.url);res.end('ok');});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 try {
  const baseURL=`http://127.0.0.1:${server.address().port}`;
  for(const [params,expected] of cases) {
   await axios.get('/witness',{baseURL,params,proxy:false});
   assert.equal(received.at(-1),'/witness'+(expected?'?'+expected:''),'physical Axios HTTP request target');
  }
  console.log(`Installed Axios ${axios.VERSION}: ${cases.length} getUri + ${received.length} physical request targets PASS`);
 } finally { await new Promise(resolve=>server.close(resolve)); }
})().catch(error=>{ console.error(error);process.exitCode=1; });
