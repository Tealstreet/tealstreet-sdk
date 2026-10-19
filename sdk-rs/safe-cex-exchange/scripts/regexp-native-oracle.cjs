const cases = {
 to_string: [
  {input:{kind:"stringBeforeNumber",units:[55296,97]}},
  {input:{kind:"stringBeforeThrow",units:[56320,98]}},
  {input:{kind:'units',units:[55296,97]}},
  {input:{kind:'objectString',units:[56320,98]}},
  {input:{kind:'throw'}},
 ],
 to_length: [
  {input:{kind:'number',number:'2.75'}},
  {input:{kind:'number',number:'-1'}},
  {input:{kind:'number',number:'NaN'}},
  {input:{kind:'objectNumber',number:'3.9'}},
  {input:{kind:'throw'}},
 ],
 split_limit: [
  {input:{kind:'number',number:'4294967297'}},
  {input:{kind:'undefined'}},
  {input:{kind:'number',number:'NaN'}},
  {input:{kind:'number',number:'-1'}},
  {input:{kind:'objectNumber',number:'2.75'}},
  {input:{kind:'throw'}},
 ],
 replace: [
  {pattern:'(a)',flags:'g',input:{kind:'units',units:[97,98,97]},replacement:{kind:'units',units:[36,38,58,36,49,58,36,36]}},
  {pattern:'(a)',flags:'g',input:{kind:'units',units:[97,98,97]},replacement:{kind:'callback',units:[55296]}},
  {pattern:'a',flags:'',input:{kind:'units',units:[97]},replacement:{kind:'throw'}},
 ],
 match_all_or_first: [
  {pattern:'(a)?b',flags:'',input:{kind:'units',units:[98]}},
  {pattern:'',flags:'g',input:{kind:'units',units:[55296,97]}},
  {pattern:'z',flags:'g',input:{kind:'units',units:[97]}},
 ],
 search: [
  {pattern:'a',flags:'g',input:{kind:'units',units:[98,97]},previous:7},
  {pattern:'z',flags:'',input:{kind:'units',units:[97]},previous:-0},
 ],
 split: [
  {pattern:'(,)',flags:'',input:{kind:'units',units:[97,44,98]},limit:{kind:'undefined'}},
  {pattern:'(,)',flags:'',input:{kind:'units',units:[97,44,98]},limit:{kind:'number',number:'2'}},
  {pattern:'(x)?b',flags:'',input:{kind:'units',units:[97,98,99]},limit:{kind:'undefined'}},
  {pattern:',',flags:'',input:{kind:'units',units:[55296,44,56320]},limit:{kind:'undefined'}},
 ],
};


const bits=n=>{const a=new DataView(new ArrayBuffer(8));a.setFloat64(0,n);return a.getBigUint64(0).toString(16).padStart(16,'0');};
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const view=v=>v===undefined?{kind:'undefined'}:v===null?{kind:'null'}:typeof v==='string'?{kind:'string',units:units(v)}:typeof v==='number'?{kind:'number',bits:bits(v)}:Array.isArray(v)?{kind:'array',items:Array.from(v,view)}:{kind:'object'};
function observe(owner,row,publicRoute=false){let calls=0;const sentinel={};const input=d=>d.kind==='units'?String.fromCharCode(...d.units):d.kind==='number'?Number(d.number):d.kind==='undefined'?undefined:d.kind==='stringBeforeNumber'||d.kind==='stringBeforeThrow'?{toString(){calls++;return String.fromCharCode(...d.units);},valueOf(){calls++;if(d.kind==='stringBeforeThrow')throw sentinel;return 7;}}:d.kind==='objectString'?{toString(){calls++;return String.fromCharCode(...d.units);}}:d.kind==='objectNumber'?{valueOf(){calls++;return Number(d.number);}}:{toString(){calls++;throw sentinel;},valueOf(){calls++;throw sentinel;}};
let re,previous,callbackArgs=[];
try{const x=input(row.input);let value;
if(publicRoute&&owner==='to_string'){re=/[\s\S]*/;value=re.exec(x);}
else if(publicRoute&&owner==='to_length'){re=/./g;re.lastIndex=x;value=re.exec('abcdef');}
else if(publicRoute&&owner==='split_limit'){re=/(?:)/;value=re[Symbol.split]('abcd',x);}
else if(owner==='to_string')value=String(x);
else if(owner==='to_length'){const n=Number(x);value=Number.isNaN(n)||n<=0?0:Math.min(Math.trunc(n),Number.MAX_SAFE_INTEGER);}
else if(owner==='split_limit')value=x===undefined?4294967295:Number(x)>>>0;
else{re=new RegExp(row.pattern,row.flags);if(owner==='replace'){const d=row.replacement;const replacement=d.kind==='callback'?function(...args){calls++;callbackArgs.push(args.map(view));return String.fromCharCode(...d.units);}:input(d);value=re[Symbol.replace](x,replacement);}
else if(owner==='match_all_or_first')value=re[Symbol.match](x);
else if(owner==='search'){previous={marker:row.previous};re.lastIndex=previous;value=re[Symbol.search](x);}
else value=re[Symbol.split](x,input(row.limit));}
return {ok:true,value:view(value),calls,...(re?{lastIndex:owner==='search'?{same:re.lastIndex===previous}:view(re.lastIndex)}:{}),...(owner==='replace'?{callbackArgs}:{} )};
}catch(error){return {ok:false,sameError:error===sentinel,calls};}}
const outputs=Object.fromEntries(Object.entries(cases).map(([owner,rows])=>[owner,rows.map(input=>({input,output:observe(owner,input)}))]));
const publicCases=Object.fromEntries(Object.entries(cases).map(([owner,rows])=>[owner,rows.map(input=>({input,output:observe(owner,input,true)}))]));
process.stdout.write(JSON.stringify({direct:outputs,publicCases},null,2)+'\n');
