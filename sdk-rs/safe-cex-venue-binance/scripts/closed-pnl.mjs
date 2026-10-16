import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const types=fs.readFileSync(new URL('packages/safe-cex/src/types.ts',root),'utf8');
const ast=ts.createSourceFile('types.ts',types,ts.ScriptTarget.Latest,true);
const side=ast.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text==='PositionSide');
assert.ok(side);
const enums={};vm.runInNewContext(ts.transpileModule(side.getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
const source=fs.readFileSync(new URL('packages/safe-cex/src/exchanges/binance/binance.closed-pnl.ts',root),'utf8');
const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:id=>{assert.equal(id,'../../types');return enums;}});
const norm=v=>typeof v==='number'?{number:Object.is(v,-0)?'-0':String(v)}:Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,norm(x)])):v;
const base={realizedPnl:'5',time:123,symbol:'BTCUSDT',price:'12',qty:'2',quoteQty:'25',side:'SELL',commission:'0.1'};
const values=[undefined,null,false,true,0,-0,-1,1,1e21,'',' ','0x10','12abc','-0','NaN','Infinity','1e999','-1e999','1e-999',[],[1],[null],['12'],['12','3'],{}, {toString:0},{toString:null},[{toString:0}],{valueOf:2},'SELL','sell','ſell','BUY','long','🦀'];
for(let i=0;i<256;i++) values.push(String.fromCharCode(i)+'12tail');
const inputs=[];
for(const key of Object.keys(base)) for(const value of values) inputs.push(JSON.stringify([{...base,[key]:value}]));
for(const key of Object.keys(base)) for(const literal of ['1e400','-1e400','-0']) inputs.push('[{'+Object.entries(base).map(([k,v])=>JSON.stringify(k)+':'+(k===key?literal:JSON.stringify(v))).join(',')+'}]');
inputs.push(JSON.stringify([]),JSON.stringify([{},base,{...base,realizedPnl:0},base]));
for(const key of Object.keys(base)) inputs.push(JSON.stringify([base,{...base,[key]:{toString:null}},base]));
// Both conversions happen before the time/symbol guard; later conversions are skipped.
inputs.push(JSON.stringify([{...base,time:0,symbol:{toString:null}}]), JSON.stringify([{...base,realizedPnl:0,time:{toString:null}}]),JSON.stringify([{...base,time:0,price:{toString:null}}]));
for(const price of ['1e308','-1e308','-0','0']) for(const qty of ['1e308','-1','-0']) for(const quoteQty of [null,'0','-0','12']) inputs.push(JSON.stringify([{...base,price,qty,quoteQty}]));
const cases=[];
for(const rows of inputs) for(const account of ['acct','é 🦀','']) {
 let expected; try{expected={value:norm(exports.deriveBinanceClosedPositions(JSON.parse(rows),account))};}catch(e){expected={error:e.name};}
 cases.push({rows,account,expected});
}
process.stdout.write(JSON.stringify(cases));
