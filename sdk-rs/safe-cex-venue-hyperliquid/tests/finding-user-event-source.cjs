'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts';
const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true),names=new Set(['WsUserEvent','WsFill','WsUserFunding','WsLiquidation','WsNonUserCancel']);
const declarations=ast.statements.filter(n=>(ts.isInterfaceDeclaration(n)||ts.isTypeAliasDeclaration(n))&&names.has(n.name.text)).map(n=>n.getText(ast)).join('\n');
function errors(body){const filename=path.join(__dirname,'finding-user-event-witness.ts'),text=declarations+'\n'+body,options={strict:true,noEmit:true,target:ts.ScriptTarget.ES2022,types:[],skipLibCheck:true};const h=ts.createCompilerHost(options),get=h.getSourceFile.bind(h);h.getSourceFile=(file,...args)=>file===filename?ts.createSourceFile(file,text,ts.ScriptTarget.ES2022,true):get(file,...args);return ts.getPreEmitDiagnostics(ts.createProgram([filename],options,h)).filter(d=>d.category===ts.DiagnosticCategory.Error)}
const rows=JSON.parse(fs.readFileSync(path.join(__dirname,'fixtures/finding-user-event-inputs.json'),'utf8'));assert.equal(rows.length,4,'EVENT independent source input floor');let completed=0,last=-1;
for(let i=0;i<4;i++){const row=rows[i],value=JSON.parse(row.raw),user='\ud800user',payload=Object.assign(value,{user});const expression=Array.isArray(value)?`Object.assign(${row.raw},{user:'user'})`:`({...${row.raw},user:'user'})`;
 assert.equal(errors(`const witness:WsUserEvent=${expression};`).length,0,'EVENT actual core declaration accepts valid alternative');
 assert(errors(`const witness:WsUserEvent=${row.raw};`).length>0,'EVENT actual core declaration requires user');
 assert.equal(payload.user.charCodeAt(0),0xd800,'EVENT source native retained user unit');assert(Object.hasOwn(payload,'user'),'EVENT source attached own user');assert.equal(Array.isArray(payload),['fills','cancels'].includes(row.kind),'EVENT source native array variant');
 completed++;last=i;
}
assert.equal(errors('const witness:WsUserEvent={user:"wallet"};').length>0,true,'EVENT actual core rejects user-only fabrication');assert.equal(completed,4,'EVENT independently completed source floor');assert.equal(last,3,'EVENT source stop3');process.stdout.write(JSON.stringify({count:completed,last}));
