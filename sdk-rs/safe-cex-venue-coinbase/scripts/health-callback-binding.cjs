const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name?.text==='CoinbaseExchange');assert.equal(cls.length,1);
const method=cls[0].members.filter(n=>n.name?.getText(ast)==='markPrivateUserStreamDegraded');assert.equal(method.length,1);
const code=ts.transpileModule(`(function(){return (${method[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const rows=[];
for(const fault of ['log','warning']){
 let warnings=0,updates=0;const error=new Error(fault+'-consumer-failed');
 const state={hasWarnedPrivateUserStreamDegraded:false,store:{updateSocketHealth(){updates++;}},log(){if(fault==='log')throw error;},emitter:{emit(){warnings++;throw error;}}};
 const fn=vm.runInNewContext(code,{LogSeverity:{Warning:'warning'}}).call(state);
 let sameError=false;try{fn('stale stream',true);}catch(caught){sameError=caught===error;}
 const first={fault,warnings,updates,sameError,warned:state.hasWarnedPrivateUserStreamDegraded};
 state.log=()=>{};state.emitter.emit=()=>{warnings++;};
 fn('stale stream',true);
 rows.push({...first,recoveryWarnings:warnings,recoveryWarned:state.hasWarnedPrivateUserStreamDegraded});
}
assert.equal(rows.length,2);process.stdout.write(JSON.stringify(rows));
