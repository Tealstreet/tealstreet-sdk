'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-private.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidPrivateWebSocket');
assert(cls,'actual private socket source class');
let text=cls.getText(source);
if(process.env.SAFE_CEX_HL_CLOSE_CONTROL==='1'){
  const original='this.ws?.close?.();';assert.equal(text.split(original).length-1,1);
  text=text.replace(original,'this.forceClose();');
}
let closes=0,superDisposes=0,forced=0;
class BaseWebSocket {
  constructor(parent){this.parent=parent;this.forceClosed=false;this.isDisposed=false;}
  forceClose(){forced++;this.forceClosed=true;this.ws?.close();}
  dispose(){superDisposes++;this.forceClosed=true;this.isDisposed=true;}
}
const context={exports:{},BaseWebSocket,Map,Set,clearTimeout(){}};
const code=ts.transpileModule(text+';exports.C=HyperliquidPrivateWebSocket;',{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}
}).outputText;
vm.runInNewContext(code,context);
const socket=new context.exports.C({});socket.ws={close(){closes++;}};
socket.dispose();
assert.equal(closes,1,'actual private dispose closes its wire');
assert.equal(superDisposes,0,'actual private dispose does not call super.dispose');
assert.equal(forced,0,'actual private dispose does not call forceClose');
assert.equal(socket.forceClosed,false,'actual private dispose preserves forceClosed');
assert.equal(socket.isDisposed,false,'actual private dispose preserves parent disposal state');
process.stdout.write(JSON.stringify({closes,superDisposes,forced,forceClosed:socket.forceClosed,isDisposed:socket.isDisposed}));
