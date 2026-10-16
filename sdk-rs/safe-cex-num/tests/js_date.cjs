const buffer=Buffer.alloc(8);
const {bits} = require('./support/number-bits.cjs');
const value=n=>{buffer.writeBigUInt64BE(BigInt.asUintN(64,n));return buffer.readDoubleBE();};
let state=42n;const random=()=>{state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);return state;};
const inputs=['','0','1','12','13','31','32','49','50','99','100','0000','0001','-000000','+000000','1970-01-01','1970-01-01T00:00','2024-02-31','2024-02-30T00:00:00Z','2024-13-01','2024-01-00','Jan 1','Feb 30 2020','garbage 2000 Jan 1','garbage2000 Jan 1','2000 Jan 1 garbage','Jan(foo(bar))1 2000','Jan 1 2000 (unfinished','Jan 1 2000)','1970-01-01\u0000invalid','1970-01-01T24:00:00.000Z','1970-01-01T24:00:00.001Z','2016-12-31T23:59:60Z','1970-01-01T00:00:00.000000000123456789123Z','+275760-09-13T00:00:00.000Z','+275760-09-13T00:00:00.001Z','-271821-04-20T00:00:00.000Z','1970-01-01 00::','1970-01-01 00:00 GMT+123456789:00','1970-01-01T00:00Zsuffix','1970-01-01T00:00z','2000-1-1','2000/1/1','2000.1.1'];
const whitespace=['\ufeff','\u0085','\u2028','\u2029','\u00a0','\t','\n','\r',' ','\u2007','\u0000','\ud800','\udfff'];
for(const w of whitespace)for(const base of ['2000-01-01','2000-01-01T00:00:00Z','Jan 1 2000'])for(const s of [w+base,base+w,base.replace(/[- ]/g,w)])inputs.push(s);
const years=['0000','0001','0099','1900','1969','1970','2000','2024','2025','9999','+010000','-000001','+275760','-271821','-999999','+999999','999999999','00000000002000'];
const months=['00','01','02','04','12','13','1','002'];const days=['00','01','28','29','30','31','32','1'];
const times=['','T00:00','T23:59:59.999','T24:00','T24:00:01','T1:00','T00:60','T00:00:60','T12:34:56.1','T12:34:56.0001',' 12:34:56',' 0:1:2',' 12::',' 12:00 PM',' 13:00 AM'];
const zones=['','Z','z','+00:00','-08:00','+0530','+24:00',' UTC',' GMT-8',' PST',' GMT+8:30'];
const pick=a=>a[Number(random()%BigInt(a.length))];
for(let i=0;i<20000;i++)inputs.push(`${pick(years)}-${pick(months)}-${pick(days)}${pick(times)}${pick(zones)}`);
const tokens=['Jan','January','Janfoo','Feb','Dec','Mon','T','am','PM','UTC','GMT','PST','Z','foo','-', '+',':','.',',','/','(',')','0','1','12','24','31','59','69','99','2000','1234567890','\ufeff','\u0085','\u2028','\ud800'];
for(let i=0;i<10000;i++){let s='';for(let j=0,n=Number(random()%9n);j<n;j++)s+=pick(tokens)+(random()%2n?' ':'');inputs.push(s);}
const clips=[NaN,Infinity,-Infinity,0,-0,0.5,-0.5,1.5,-1.5,8.64e15,-8.64e15,8.64e15+1,-8.64e15-1];
for(let i=0;i<10000;i++)clips.push(value(random()));
console.error(JSON.stringify({node:process.version,tz:process.env.TZ,parse:inputs.length,clip:clips.length}));
console.log(JSON.stringify({parse:inputs.map(input=>({units:Array.from({length:input.length},(_,i)=>input.charCodeAt(i)),expected:bits(Date.parse(input))})),clip:clips.map(x=>[bits(x),bits(new Date(x).getTime())])}));
