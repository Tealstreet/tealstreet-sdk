const texts={ascii:'a,b,c',empty:'',repeated:',a,,',emoji:'a😀b😀c',surrogate:'a\ud800b\udc00c'};
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const separators={comma:()=>',',empty:()=>'',undefined:()=>undefined,null:()=>null,emoji:()=>'😀',high:()=>'\ud800',low:()=>'\udc00',object:log=>({toString(){log.push('separator');return ','}}),throwing:log=>({toString(){log.push('separator');throw new URIError('separator')}}),regex_capture:()=>/(,)/,regex_optional:()=>/(,)|(x)/,regex_empty:()=>/(?:)/};
const limits={undefined:()=>undefined,zero:()=>0,one:()=>1,two:()=>2,three:()=>3,negative_one:()=>-1,negative_two:()=>-2,fraction:()=>2.9,negative_fraction:()=>-0.5,negative_zero:()=>-0,nan:()=>NaN,infinity:()=>Infinity,negative_infinity:()=>-Infinity,wrap_zero:()=>2**32,wrap_one:()=>2**32+1,negative_wrap:()=>-(2**32)+2,large_signed:()=>2**63,large_unsigned:()=>2**64,huge:()=>1e100,string:()=>'2',object:log=>({valueOf(){log.push('limit');return 2}}),throwing:log=>({valueOf(){log.push('limit');throw new RangeError('limit')}}),symbol:()=>Symbol.iterator,bigint:()=>2n};
const cases=[];
for(const [textTag,text] of Object.entries(texts))for(const [separatorTag,separator] of Object.entries(separators))for(const [limitTag,limit] of Object.entries(limits)){
 const log=[];let result=null,error=null;
 try{result=text.split(separator(log),limit(log)).map(v=>v===undefined?{type:'undefined'}:{type:'string',units:units(v)})}catch(e){error=e.name}
 cases.push({textTag,source:units(text),separatorTag,limitTag,result,error,log});
}
process.stdout.write(JSON.stringify({cases}));
