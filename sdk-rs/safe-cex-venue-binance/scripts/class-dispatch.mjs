import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import{createRequire}from'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=new URL('../../../',import.meta.url);
const read=p=>ts.createSourceFile(p,fs.readFileSync(new URL(p,root),'utf8'),ts.ScriptTarget.Latest,true);
const ast=read('packages/safe-cex/src/exchanges/binance/binance.ws-public.ts');
const declaration=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='dispatchBinanceMessage'));assert.ok(declaration);
const parser=read('packages/json-parse/src/jsonParse.ts').statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText()==='jsonParse'));assert.ok(parser);
const source=ts.transpileModule(`${parser.getText()}
${declaration.getText()}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const input=JSON.parse(fs.readFileSync(0,'utf8')),output=[];
for(const c of input){const context=vm.createContext({exports:{}});vm.runInContext(source,context);context.input=c;
output.push(JSON.parse(vm.runInContext(`(()=>{const c=input,trace=[],handlers={},base={};let first;
const add=(key,name,where=handlers)=>{const callback=events=>{trace.push(name);if(name==='broadcast'&&c.live){first.push(e=>trace.push('appended'));events[0].native=4.2}if(name==='exact'&&c.live){if(events[0].native!==4.2)throw Error('shared native payload')}};const list=[callback];where[key]=list;if(name==='broadcast')first=list};
if(c.broadcast)add(c.event,'broadcast');if(c.exact)add(c.event+'.BTC','exact');if(c.prefix)add(c.event+'.1m.ETH','prefix');if(c.inherited){add(c.event+'.1m.ETH','prefix',base);Object.setPrototypeOf(handlers,base)}
if(c.badPrefix)handlers[c.event+'.bad']=null;
let failure=null;try{exports.dispatchBinanceMessage(handlers,c.data,event=>{trace.push('stats:'+event);if(c.statsError)throw new RangeError('stats fixture');if(c.clear)first.splice(0)})}catch(e){failure={name:e.name,message:e.message}}return JSON.stringify({trace,failure})})()`,context)));
}process.stdout.write(JSON.stringify(output));
