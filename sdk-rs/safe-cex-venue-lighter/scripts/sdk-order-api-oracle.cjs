const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file=path.resolve(__dirname,'../../../packages/lighter-sdk/src/client/order-api.ts');
const mutation=process.env.LIGHTER_ORDER_API_MUTATION;
let source=fs.readFileSync(file,'utf8');
if(mutation==='missing-import') source=source.replace("import { OrderStatus } from '../types/constants';",'');
if(mutation==='heritage') source=source.replace('interface ApiOrder {','interface ApiOrder extends Extra {')+'\ninterface Extra { hidden:number; }';
if(mutation==='response-heritage') source=source.replace('interface AccountOrdersResponse {','interface AccountOrdersResponse extends Extra {')+'\ninterface Extra { hidden:number; }';
if(mutation==='status-type') source=source.replace('status: OrderStatus;','status: number;');
const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
assert.equal(tree.parseDiagnostics.length,0,'order API syntax diagnostics');
const imports=tree.statements.filter(n=>ts.isImportDeclaration(n)&&n.moduleSpecifier.text==='../types/constants');
assert.equal(imports.length,1,'order API OrderStatus import inventory');
assert.equal(imports[0].importClause.getText(tree),'{ OrderStatus }','order API OrderStatus import binding');
const names=['ApiOrder','AccountOrdersResponse'];
const declarations=names.map(name=>{
 const found=tree.statements.filter(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);
 assert.equal(found.length,1,`${name} declaration inventory`);
 assert.ok(!found[0].heritageClauses?.length,`${name} unsupported interface heritage`);return found[0];
});
const expected={ApiOrder:[['orderId','string'],['marketId','number'],['symbol','string'],['accountIndex','number'],['price','string'],['quantity','string'],['filledQuantity','string'],['remainingQuantity','string'],['isBuy','boolean'],['postOnly','boolean'],['reduceOnly','boolean'],['createdAt','number'],['updatedAt','number'],['status','OrderStatus']],AccountOrdersResponse:[['orders','ApiOrder[]'],['total','number',true],['nextCursor','string',true]]};
const schemas=Object.fromEntries(declarations.map(n=>[n.name.text,n.members.map(f=>[f.name.text,f.type.getText(tree),!!f.questionToken])]));
for(const name of names)assert.deepEqual(schemas[name],expected[name].map(([n,t,o])=>[n,t,!!o]),`${name} source schema`);
const selected=imports[0].getFullText(tree)+declarations.map(n=>n.getFullText(tree)).join('\n');
const virtual=path.join(path.dirname(file),'__order_record_probe__.ts');
const host=ts.createCompilerHost({});const read=host.readFile.bind(host),exists=host.fileExists.bind(host);
host.readFile=f=>f===virtual?selected:mutation==='missing-enum'&&f.endsWith('/types/constants.ts')?read(f).replace('export enum OrderStatus','export enum RemovedStatus'):read(f);
host.fileExists=f=>f===virtual||exists(f);
const program=ts.createProgram([virtual],{target:ts.ScriptTarget.ES2022,strictNullChecks:true,skipLibCheck:true},host);
const diagnostics=ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length,0,`order API selected program diagnostics must be empty: ${diagnostics.length}`);
const base={orderId:'order-A',marketId:11.25,symbol:'SYMBOL-B',accountIndex:22.5,price:'31.25',quantity:'42.5',filledQuantity:'53.75',remainingQuantity:'64.125',isBuy:true,postOnly:false,reduceOnly:true,createdAt:75.5,updatedAt:86.75,status:2};
const orders=Array.from({length:8},(_,i)=>({...base,orderId:`order-${i}`,isBuy:!!(i&1),postOnly:!!(i&2),reduceOnly:!!(i&4)}));
if(mutation==='content')orders[0].price='wrong-price';
if(mutation==='reverse')orders.reverse();
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(n)?NaN:n);return b.toString('hex');};
const lexemes=['-0','-0.0','-0e0','0','1e3','1e400','-1e400','16777217.25','-1e-400'];
const numeric=[];
for(const field of ['marketId','accountIndex','createdAt','updatedAt','status'])for(const raw of lexemes){
 const encoded=JSON.stringify({...base,[field]:'__NUMBER__'}).replace('"__NUMBER__"',raw);
 numeric.push({field,raw,encoded,bits:bits(JSON.parse(encoded)[field])});
}
const totals=lexemes.map(raw=>{const encoded=`{"orders":[],"total":${raw}}`;return {raw,encoded,bits:bits(JSON.parse(encoded).total)};});
const texts=['',' x ','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'];
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const cursors=texts.map(text=>{const encoded=JSON.stringify({orders:[],nextCursor:text});const value=JSON.parse(encoded).nextCursor;return {encoded,units:units(value),representable:value.isWellFormed()};});
const extras=[];
for(const record of names)for(const nested of record==='ApiOrder'?[false]:[false,true])for(const text of texts){
 const input=record==='ApiOrder'?structuredClone(base):{orders:[structuredClone(base)]};(nested?input.orders[0]:input).extra=text;
 const encoded=JSON.stringify(input),parsed=JSON.parse(encoded),value=(nested?parsed.orders[0]:parsed).extra;
 extras.push({record,nested,encoded,units:units(value),type:typeof value,length:value.length,representable:value.isWellFormed()});
}
const strings=[];
for(const field of ['orderId','symbol','price','quantity','filledQuantity','remainingQuantity'])for(const text of ['', ' x ', 'é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y']){
 const encoded=JSON.stringify({...base,[field]:text}); const actual=JSON.parse(encoded)[field];
 strings.push({field,encoded,representable:actual.isWellFormed(),units:Array.from({length:actual.length},(_,i)=>actual.charCodeAt(i))});
}
const depth=[];
for(const record of names)for(const nested of record==='ApiOrder'?[false]:[false,true])for(const count of [124,125,126,127,140]){
 const input=record==='ApiOrder'?structuredClone(base):{orders:[structuredClone(base)]};let extra=17;for(let i=0;i<count;i++)extra=[extra];
 (nested?input.orders[0]:input).extra=extra;const encoded=JSON.stringify(input),parsed=JSON.parse(encoded);let leaf=(nested?parsed.orders[0]:parsed).extra,seen=0;while(Array.isArray(leaf)){assert.equal(leaf.length,1);seen++;leaf=leaf[0];}assert.equal(seen,count);assert.equal(leaf,17);
 depth.push({record,nested,count,encoded});
}
// Change the escaped UTF16 unit without changing encoded length or metadata.
if(mutation==='surrogate-content') {
  const original = strings[6].encoded;
  strings[6].encoded = original.replace('\\ud800', '\\ud801');
  assert.notEqual(strings[6].encoded, original, 'ORDER-API surrogate control changes bytes');
  assert.equal(strings[6].encoded.length, original.length, 'ORDER-API surrogate control preserves length');
}
const body={schemas,base,orders,numeric,strings,depth,totals,cursors,extras};
const contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex');
assert.equal(contentHash,'e0117b9257f0a13eebe4814e37a12847dd425d1e64e31436309b8bb414881bdb','ORDER-API live corpus content');
process.stdout.write(JSON.stringify({header:{nodeVersion:process.version,captureArchitecture:process.arch,capturePlatform:process.platform,scope:'Selected exact source interface/import declarations; no runtime OrderApi compilation'},contentHash,...body}));
