const raw=[null,'null','false','true','0','-0','1.5','1e400','-1e400','[]','[null]','[[""]]','{}','{"toString":null}','[{"toString":null}]'];
const strings=['',' ','\ufeff','\u0085','1.0','1e3','Infinity','NaN','0x','0b2','+0x1','-0b1','1_000','１２','+','-','--1','0o777','0Xff'];
for(const n of [999,1000,1001]) strings.push('x'.repeat(n));
strings.push('😀'.repeat(501),'x'+'😀'.repeat(500));
for(let n=-200;n<=200;n++) {
 raw.push(String(n),String(n+0.5));
 for(const x of [String(n),'+'+n,'0x'+n.toString(16),'0b'+n.toString(2),'0o'+n.toString(8)]) strings.push(x,'\ufeff'+x+'\ufeff');
}
for(let e=-324;e<=308;e++)raw.push('1e'+e,'-1e'+e);
for(const s of strings)raw.push(JSON.stringify(s),JSON.stringify([s]));
const rows=raw.map(raw=>({raw}));
for(const primitive of ['NaN','Infinity','-Infinity','Symbol()','10n'])rows.push({primitive});
process.stdout.write(JSON.stringify(rows.map(row=>{
 const value=row.primitive ? Function('return '+row.primitive)() : row.raw===null?undefined:JSON.parse(row.raw);
 try{return {...row,expected:{ok:String(BigInt(value))}};}catch(e){return {...row,expected:{error:{name:e.name,message:e.message.toWellFormed(),units:Array.from({length:e.message.length},(_,i)=>e.message.charCodeAt(i))}}};}
})));
