const fs = require('node:fs'), path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root} = require('./tables.cjs');
function method(venue,name,receiver,clock,reconnecting) {
 const file = path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.ws-private.ts`);
 const ast = ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const owner = ast.statements.find(ts.isClassDeclaration);
 const member = owner.members.find(n => n.name?.getText(ast) === name);
 if (!member?.initializer) throw Error('missing maintained '+venue+'.'+name);
 const code = ts.transpileModule('module.exports='+member.initializer.getText(ast), {
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
 }).outputText;
 const module = {exports:{}};
 new Function('module','Date','ENABLE_RECONNECTING_WEBSOCKET',code).call(receiver,module,clock,reconnecting);
 return module.exports;
}
const input = JSON.parse(fs.readFileSync(0,'utf8'));
const captures=[];
for(const c of input) {
 const events=[],frames=[],failure=new TypeError('private operation failure');
 let reads=0;
 const receiver={isDisposed:c.disposed,subscribed:c.subscribed};
 const clock={now(){events.push('date');return 123+reads++;}};
 let error=null;
 if(c.kind==='subscribe') {
  receiver.sendWithLog=frame=>{
   events.push('send');const index=frames.length;frames.push(frame);
   if(index===c.failAt)throw failure;
  };
 } else {
  const call=(name,arg)=>{
   events.push(arg===undefined ? name : name+':'+arg);
   if(name==='subscribe' && c.venue==='kucoin') {
    if(receiver.subscribed)throw Error('futures onOpen did not reset subscribed');
    receiver.subscribed=true;
   }
   if(c.failure===name)throw failure;
  };
  receiver.subscribe=()=>call('subscribe');receiver.ping=()=>call('ping');
  receiver.parent={fetchAndSetPrivateSnapshot:arg=>call('snapshot',arg)};
 }
 try {method(c.venue,c.kind==='subscribe'?'subscribe':'onOpen',receiver,clock,c.reconnecting)();}
 catch(e){error={name:e.name,message:e.message,same:e===failure};}
 captures.push({events,frames,subscribed:receiver.subscribed,error});
}
process.stdout.write(JSON.stringify(captures));
