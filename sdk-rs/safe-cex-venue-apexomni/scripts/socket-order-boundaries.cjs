// Reuse unchanged socket/parent source loaders and actual shared DefaultStore.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.resolve(__dirname,'websockets.cjs');let code=fs.readFileSync(filename,'utf8');
if(process.env.SAFE_CEX_APEX_SOCKET_ORDER_CONTROL){
 assert.equal(process.env.SAFE_CEX_APEX_SOCKET_ORDER_CONTROL,'predicate','APEX-WS-ORDER named source control');
 const hook='const code=ts.transpileModule(declaration,';
 assert.equal(code.split(hook).length,2,'APEX-WS-ORDER actual socket compiler selector');
 code=code.replace(hook,()=>String.raw`const code=ts.transpileModule((()=>{
  if(name!=='private')return declaration;
  const ast=ts.createSourceFile('socket-order-control.ts',declaration,ts.ScriptTarget.Latest,true);
  const cls=ast.statements.find(ts.isClassDeclaration);
  const member=cls.members.find(n=>n.name?.getText(ast)==='handleOrderTopic');
  assert(member,'APEX-WS-ORDER actual member');const calls=[];
  function visit(n){if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.getText(ast)==='data.some')calls.push(n);ts.forEachChild(n,visit);}
  visit(member);assert.equal(calls.length,1,'APEX-WS-ORDER unique source some');
  const value=calls[0].arguments[0].body.right;
  assert(ts.isStringLiteral(value)&&value.text==='CANCELED','APEX-WS-ORDER actual source predicate');
  return declaration.slice(0,value.getStart(ast))+"'OPEN'"+declaration.slice(value.end);
 })(),`);
}

const driver="main().catch(error=>{console.error(error);process.exitCode=1;});";
assert.equal(code.split(driver).length,2,'APEX-WS-ORDER maintained driver selector');
code=code.replace(driver,()=>String.raw`
(async()=>{
 const noBrackets={openSlParams:{triggerPrice:0},openTpParam:{triggerPrice:0}};
 const canceled={...order,...noBrackets,status:'CANCELED',updatedAt:clock,cancelReason:'USER_CANCELED'};
 const open={...canceled,status:'OPEN'};
 const cases=[
 ['canceled-null',[canceled,null]],['canceled-open-null',[canceled,{...open,id:'open-2'},null]],
 ['open-null',[open,null]],['null-canceled',[null,canceled]],['empty',[]],['open',[open]],
 ['cached-string-null',[canceled,null]],['cached-numeric',[{...canceled,id:17}]],
 ['null-root',null],['object-root',{}],['number-root',17],['undefined-root',undefined],
 ['cancel-stale',[{...canceled,cancelReason:'ACCOUNT_BALANCE_INSUFFICIENT'}]],
 ['cancel-young-connect',[{...canceled,updatedAt:clock+6000,cancelReason:'ACCOUNT_BALANCE_INSUFFICIENT'}]],
 ];
 const rows=[];
 for(const [label,input]of cases){
  socketPrivate=true;connected=true;disposed=false;nextTimer=1;timers=new Map();pingTimer=undefined;operations=[];
  const state=store();const parent=new Class('apexomni',{key:'key',secret:'secret'},state);
  state.addOrUpdateOrdersFull(parent.mapOrder(order));
  if(label==='cached-string-null')parent.recentlyCancelledOrders.set('order-1',clock);
  if(label==='cached-numeric')parent.recentlyCancelledOrders.set('17',clock);
  const socket=new Private(parent);socket.lastConnectTime=clock-(label==='cancel-young-connect'?4000:6000);
  let virtual=0,dates=0;context.virtualClock.getCurrentTimeMs=()=>{virtual++;return clock};
  const dateOffset=label.startsWith('cancel-')?6000:0;
  wsContext.Date=class extends Date{static now(){dates++;return clock+dateOffset}};
  operations=[];let result;
  try{socket.handleOrderTopic(input);result={ok:true};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({label,input:native(input),result,operations:native(operations),orders:native(state.orders),
             cache:native([...parent.recentlyCancelledOrders]),virtual,dates,timers:timers.size,dateOffset,lastConnect:socket.lastConnectTime});
 }
 assert.equal(rows.length,14,'APEX-WS-ORDER independent source floor');
 console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(code,filename);
