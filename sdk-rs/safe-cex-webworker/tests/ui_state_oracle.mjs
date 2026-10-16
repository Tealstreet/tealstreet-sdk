// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
const base=ast('exchanges/base.ts'),cls=base.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BaseExchange');
const normalizer=base.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(base)==='normalizeSymbols')).getText(base);
const methods=['getActiveTickerSymbols','setActiveTickerSymbols'].map(name=>{const member=cls.members.find(m=>m.name?.getText(base)===name);if(!member)throw Error(`missing ${name}`);return member.getText(base);}).join('\n');
const handler=ast('webworker/handlers/UiStateHandler.ts');
const source=handler.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(handler)).join('\n').replace(/\bexport /g,'');
const context=vm.createContext({BaseHandler:class{constructor(exchange){this._exchange=exchange;}}});
vm.runInContext(ts.transpileModule(normalizer+`\nclass FixtureExchange { _activeTickerSymbols=[]; events=[]; emitter={emit:(event,symbols)=>this.events.push({event,symbols:[...symbols]})}; ${methods} }\n`+source+'\nglobalThis.Exchange=FixtureExchange;globalThis.Handler=UiStateHandler;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const whitespace=['',' ','\t\n','\uFEFF','\u0085','\u2007','\u2028','BTC',' BTC ','BTC\uFEFF','𐐀','constructor','__proto__'];
const inputs=[undefined,null,false,true,0,'BTC',[],{}, {activeSymbols:null},{activeSymbols:'BTC'},{activeSymbols:{}},{activeSymbols:[]},{activeSymbols:[1,true,null,{},[]]},...whitespace.map(s=>({activeSymbols:[s,'ETH',s]})),{activeSymbols:whitespace}];
const rows=[];
for(const initial of [[],['BTC'],['ETH','BTC'],[' BTC ','𐐀']])for(const input of inputs){
 const exchange=new context.Exchange();exchange.setActiveTickerSymbols(initial);exchange.events=[];
 const handler=new context.Handler(exchange),steps=[input,{activeSymbols:['NEXT','BTC','NEXT']},input,{activeSymbols:[]}];
 const expected=steps.map(data=>{handler.handleActiveSymbolsChange(data);return {symbols:[...exchange.getActiveTickerSymbols()],events:structuredClone(exchange.events)};});
 rows.push({initial,steps:steps.map(data=>({data})),expected});
}
const inherited=new context.Exchange();new context.Handler(inherited).handleActiveSymbolsChange(Object.create({activeSymbols:['INHERITED']}));
const namedArray=[];namedArray.activeSymbols=['NAMED'];const array=new context.Exchange();new context.Handler(array).handleActiveSymbolsChange(namedArray);
const surrogateInput={activeSymbols:['\uD800']},surrogate=new context.Exchange();new context.Handler(surrogate).handleActiveSymbolsChange(surrogateInput);
process.stdout.write(JSON.stringify({rows,witness:{inherited:inherited.getActiveTickerSymbols(),array:array.getActiveTickerSymbols(),surrogateInput:JSON.stringify(surrogateInput),surrogateOutput:JSON.stringify(surrogate.getActiveTickerSymbols())}}));
