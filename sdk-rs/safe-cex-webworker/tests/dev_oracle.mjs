// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/handlers/DevHandler.ts',import.meta.url);
const source=fs.readFileSync(path,'utf8'),ast=ts.createSourceFile('DevHandler.ts',source,ts.ScriptTarget.Latest,true);
const code=ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');
const rows=[];
for(const op of ['dev:simulateWebSocketsReconnect','dev:simulateHOWClose','dev:forceOpen','dev:forceClose','init'])for(const mask of [0,1,85,170,255])for(const fail of [-1,0,3,7]){
 const events=[],closed=Array(8).fill(false),closes=Array(8).fill(0);
 const sockets=closed.map((_,id)=>mask&(1<<id)?{
  reconnect(){closes[id]++;},forceClose(){closed[id]=true;},
  forceOpen(){closed[id]=false;events.push(`open:${id}`);if(id===fail)throw Error('fixture failure');}
 }:undefined);
 const context=vm.createContext({BaseHandler:class{constructor(){this._exchange={getAllSockets:()=>sockets};}},howWebSocketJA:{simulateClose(){events.push('ja');}},howWebSocketSG:{simulateClose(){events.push('sg');}}});
 vm.runInContext(ts.transpileModule(code+'\nglobalThis.Handler=DevHandler;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 let error=null;try{new context.Handler().handle({op});}catch(e){error=e.message;}
 rows.push({op,mask,fail,events,closed,closes,error});
}
process.stdout.write(JSON.stringify(rows));
