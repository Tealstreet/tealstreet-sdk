import {execFileSync} from 'node:child_process';
const spec=JSON.parse(execFileSync('node',[new URL('../../safe-cex-types/tests/position_record_oracle.cjs',import.meta.url).pathname],{encoding:'utf8'}));
const base={key:'BTC:long:1',account:'a',symbol:'BTC',settleCurrency:'USDT',side:'long',positionSide:'both',state:'closed',closedSize:2,avgEntryPrice:100,realizedPnl:4,fees:1,funding:-1,fundingAttributed:false,provenance:'reported',sourceAxis:'closed-positions',closeCount:3};
const rows=[];
for(const field of spec.fields.filter(f=>f.type==='number'))for(const spelling of ['-0','-0.0','1.5','1e3','1e400','-1e400','-1e-4000']){
 const raw=JSON.stringify({...base,[field.name]:'NUMBER'}).replace('"NUMBER"',spelling);
 const value=JSON.parse(raw)[field.name];const b=new DataView(new ArrayBuffer(8));b.setFloat64(0,value);rows.push({field:field.name,raw,bits:b.getBigUint64(0).toString(16).padStart(16,'0')});
}
process.stdout.write(JSON.stringify(rows));
