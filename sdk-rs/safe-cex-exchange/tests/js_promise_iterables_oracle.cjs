'use strict';
(async()=>{
 const rows=[];
 for(const method of ['all','allSettled'])for(const mode of ['array','map-values','set','string','map-entries','empty','rejected-set','noniterable','next-throw']){
  let input;
  switch(mode){
   case 'array':input=[Promise.resolve(1),Promise.resolve(2)];break;
   case 'map-values':input=new Map([['a',Promise.resolve(1)],['b',Promise.resolve(2)]]).values();break;
   case 'set':input=new Set([Promise.resolve(1),Promise.resolve(2)]);break;
   case 'string':input='a😀';break;
   case 'map-entries':input=new Map([['a',1],['b',2]]);break;
   case 'empty':input=new Set();break;
   case 'rejected-set':input=new Set([Promise.resolve(1),Promise.reject('sentinel')]);break;
   case 'noniterable':input={};break;
   case 'next-throw':input=new Map().values();input.next=()=>{throw new RangeError('iterator sentinel')};break;
  }
  let result;
  try{result={fulfilled:await Promise[method](input)}}catch(error){result={rejected:typeof error==='string'?error:{name:error.name,...(error.name==='RangeError'?{message:error.message}:{})}}}
  rows.push({method,mode,...result});
 }
 process.stdout.write(JSON.stringify(rows));
})();
