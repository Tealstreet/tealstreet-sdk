'use strict';
const cases=[];
for(const type of ['Map','Set'])for(const mode of ['delete-add','delete-current','clear-readd','update-future','revisit','nested','throw']){
 const collection=type==='Map'?new Map([[1,'A'],[2,'B']]):new Set([1,2]);
 const add=(key,value)=>type==='Map'?collection.set(key,value):collection.add(key);
 const trace=[];let changed=false,error=null;
 try{collection.forEach((value,key)=>{
  trace.push([key,value]);
  if(changed)return;changed=true;
  switch(mode){
   case 'delete-add':collection.delete(2);add(3,'C');break;
   case 'delete-current':collection.delete(1);add(3,'C');break;
   case 'clear-readd':collection.clear();add(3,'C');break;
   case 'update-future':add(2,'B2');break;
   case 'revisit':collection.delete(1);add(1,'A2');break;
   case 'nested':collection.forEach((v,k)=>trace.push(['inner',k,v]));break;
   case 'throw':throw new RangeError('callback sentinel');
  }
 });}catch(e){error={name:e.name,message:e.message};}
 cases.push({type,mode,trace,error});
}
process.stdout.write(JSON.stringify(cases));
