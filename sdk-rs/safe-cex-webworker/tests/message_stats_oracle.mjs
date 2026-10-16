// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/webworker/postProcessors/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const context=vm.createContext({});vm.runInContext(ts.transpileModule(source('ExchangeDependentPostprocessor.ts')+'\n'+source('MessageHandlerStatsPostprocessor.ts')+'\nglobalThis.Processor=MessageHandlerStatsPostprocessor;globalThis.has=websocketHasStats;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const names=['publicWebsocket','businessWebsocket','futuresWebsocket','privateWebsocket','publicInverseWebsocket','publicWebsocketSecondary'];
const stat=n=>({lastMessageTimestamp:n,lastCallbacksCount:n,hadHandlers:n>0});
const states=[undefined,{isInitialized:false,messageHandlersStats:{}},{isInitialized:true,messageHandlersStats:{}},{isInitialized:false,messageHandlersStats:{shared:stat(1),extra:stat(2)}}];
const inputs=[];
for(const name of names)for(const state of states)for(const existing of [undefined,{}, {public:{stale:stat(9),shared:stat(8)}},{business:{stale:stat(7)}}])inputs.push({sockets:{[name]:state},existing});
for(const primary of states)for(const inverse of states)for(const secondary of states)inputs.push({sockets:{publicWebsocket:primary,publicInverseWebsocket:inverse,publicWebsocketSecondary:secondary?{...secondary,messageHandlersStats:{shared:stat(3),secondary:stat(4)}}:undefined},existing:{public:{stale:stat(9)}}});
const rows=inputs.map(input=>{const store={messageHandlerStats:structuredClone(input.existing)},exchange=structuredClone(input.sockets);new context.Processor('a','bybit',{},exchange).process(store);return {input,expected:store.messageHandlerStats,has:names.map(n=>context.has(exchange[n]))};});
const ws={isInitialized:false,messageHandlersStats:{topic:stat(1)}};const aliasStore={};new context.Processor('a','bybit',{}, {publicWebsocket:ws}).process(aliasStore);ws.messageHandlersStats.topic.lastMessageTimestamp=99;
const partial={};new context.Processor('a','bybit',{}, {publicWebsocket:{isInitialized:false,messageHandlersStats:{topic:{lastMessageTimestamp:1}}}}).process(partial);
const nonfinite={};new context.Processor('a','bybit',{}, {publicWebsocket:{isInitialized:false,messageHandlersStats:{topic:{...stat(1),lastMessageTimestamp:Infinity}}}}).process(nonfinite);
const inherited={isInitialized:false,messageHandlersStats:Object.create({inherited:stat(1)})};
const arrayStore={messageHandlerStats:[]};new context.Processor('a','bybit',{}, {publicWebsocket:ws}).process(arrayStore);
const primaryShapes=[true,1,'xy',[],null,false,0,''].map(primary=>{const store={messageHandlerStats:{public:primary}};new context.Processor('a','bybit',{}, {publicWebsocketSecondary:{messageHandlersStats:{secondary:stat(4)}}}).process(store);return {primary,expected:store.messageHandlerStats};});
const optionalShapes=[];for(const lastCallbacksCount of [undefined,0,2])for(const hadHandlers of [undefined,false,true]){const topic={lastMessageTimestamp:1,...(lastCallbacksCount===undefined?{}:{lastCallbacksCount}),...(hadHandlers===undefined?{}:{hadHandlers})};const store={};new context.Processor('a','bybit',{}, {publicWebsocket:{messageHandlersStats:{topic}}}).process(store);optionalShapes.push({topic,expected:store.messageHandlerStats});}
process.stdout.write(JSON.stringify({rows,primaryShapes,optionalShapes,witness:{aliasedTimestamp:aliasStore.messageHandlerStats.public.topic.lastMessageTimestamp,partial:partial.messageHandlerStats,infinite:nonfinite.messageHandlerStats.public.topic.lastMessageTimestamp===Infinity,inheritedHas:context.has(inherited),arrayNamedGroup:!!arrayStore.messageHandlerStats.public}}));
