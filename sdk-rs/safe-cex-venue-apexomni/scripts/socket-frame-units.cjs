// Actual source JSON parsing, pong transmission and instrument ticker lookup.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'websockets.cjs');let code=fs.readFileSync(filename,'utf8');
if(process.env.SAFE_CEX_APEX_SOCKET_FRAME_CONTROL){
 const kind=process.env.SAFE_CEX_APEX_SOCKET_FRAME_CONTROL;
 assert(['private','public'].includes(kind),'APEX-WS-FRAME named source control');
 const hook='const code=ts.transpileModule(declaration,';
 assert.equal(code.split(hook).length,2,'APEX-WS-FRAME actual socket compiler selector');
 code=code.replace(hook,()=>String.raw`const code=ts.transpileModule((()=>{
  if(name!==process.env.SAFE_CEX_APEX_SOCKET_FRAME_CONTROL)return declaration;
  const ast=ts.createSourceFile('socket-frame-control.ts',declaration,ts.ScriptTarget.Latest,true);
  const cls=ast.statements.find(ts.isClassDeclaration),member=cls.members.find(n=>n.name?.getText(ast)==='onMessage');
  assert(member,'APEX-WS-FRAME actual onMessage');const args=[];
  function visit(n){if(ts.isCallExpression(n)&&n.expression.getText(ast)==='jsonParse'&&n.arguments[0]?.getText(ast)==='data')args.push(n.arguments[0]);ts.forEachChild(n,visit);}
  visit(member);assert.equal(args.length,1,'APEX-WS-FRAME unique actual parser argument');
  const arg=args[0],replacement="data.replaceAll('\\\\ud800','\\\\ud801')";
  return declaration.slice(0,arg.getStart(ast))+replacement+declaration.slice(arg.end);
 })(),`);
}

const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,'APEX-WS-FRAME maintained driver selector');
code=code.replace(driver,()=>String.raw`
(async()=>{
 const units=value=>Array.from({length:value.length},(_,index)=>value.charCodeAt(index));
 const cases=[['high',String.fromCharCode(0xd800)],['low',String.fromCharCode(0xdc00)],
              ['high-tail',String.fromCharCode(0xd800)+'A'],['pair','😀'],['scalar','é'],
              ['replacement',String.fromCharCode(0xfffd)],['literal',String.fromCharCode(92)+'ud800'],['unused-extra','ASCII']];
 let frames=[];const originalSend=SocketBase.prototype.send;
 SocketBase.prototype.send=function(frame){frames.push(frame);return originalSend.call(this,frame);};
 const rows=[];
 for(const privacy of ['private','public'])for(const [label,value]of cases){
  socketPrivate=privacy==='private';disposed=false;connected=true;operations=[];frames=[];
  const state=store();
  if(!socketPrivate)state.update({tickers:[{...ticker,id:value}]});
  const parent=new Class('apexomni',{key:'key',secret:'secret'},state);
  const socket=new(socketPrivate?Private:Public)(parent);
  const rawTicker=JSON.stringify(state.tickers[0]);
  const packet=socketPrivate?{op:'ping',args:[value]}:{topic:'instrumentInfo.all',data:[{s:value,p:'117.25'}]};
  if(label==='unused-extra')packet.ignored=String.fromCharCode(0xd800);
  const raw=JSON.stringify(packet);
  frames=[];operations=[];
  let result;
  try{socket.onMessage({data:raw});result={ok:true};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  const sent=frames.map(frame=>{const decoded=JSON.parse(frame);return {op:decoded.op,argumentType:typeof decoded.args?.[0],argumentUnits:units(decoded.args[0]),raw:frame};});
  const current=state.tickers[0];
  rows.push({privacy,label,inputUnits:units(value),raw,rawTicker,result,sent,
             operations:native(operations.filter(operation=>operation[0]!=='send')),
             ticker:{idUnits:units(current.id),symbol:current.symbol,last:native(current.last)}});
 }
 assert.equal(rows.length,16,'APEX-WS-FRAME independent source floor');
 console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
