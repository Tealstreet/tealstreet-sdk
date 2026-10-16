const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=name=>{const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/polymarket',name),'utf8');return ts.createSourceFile(name,text,99,true);};
const api=read('polymarket.api.ts'),constants=read('polymarket.constants.ts');
const method=api.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='PolymarketAPI').members.find(n=>n.name?.text==='getServerTime');
const endpoints=constants.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(constants)==='ENDPOINTS'));
const context={exports:{}};vm.runInNewContext(ts.transpileModule(endpoints.getText(constants)+`;class Selected { ${method.getText(api)} };exports.Selected=Selected;`,{compilerOptions:{target:99,module:1}}).outputText,context);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
(async()=>{const cases=[];for(let index=0;index<512;index++){
 const value=[index*123.25,-index*0.125,-0,index/1024,1e21+index*1048576,1e-300*(index+1),1.7e308,0][index%8];
 const calls=[],error={marker:index},receiver=new context.exports.Selected();
 receiver.xhr={get:async(url)=>{calls.push(url);if(index%8===7)throw error;return{data:value};}};
 const future=receiver.getServerTime(),immediateCalls=calls.length;
 let actual,preserved=false;try{actual=bits(await future);}catch(e){assert.strictEqual(e,error,'HTTP source original failure');preserved=e===error;}
 cases.push({index,inputBits:bits(value),outputBits:actual,preserved,calls,immediateCalls});
}process.stdout.write(JSON.stringify({cases}));})().catch(e=>{console.error(e);process.exitCode=1;});
