const fs=require('node:fs');
const {root,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const filename=root+'/packages/safe-cex/src/exchanges/registry.native.ts';
const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=source.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).filter(d=>d.name.getText(source)==='load');
if(declarations.length!==1||!declarations[0].initializer)throw Error('REGISTRY native load declaration');
const text='export const load = '+declarations[0].initializer.getText(source)+';';
const code=ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const moduleCapture={exports:{}};new Function('module','exports',code)(moduleCapture,moduleCapture.exports);
const load=moduleCapture.exports.load;
(async()=>{
 const events=[];function Constructor(){};
 const wrapped=load(()=>{events.push('loader');return Constructor;});
 events.push('created');const p=wrapped();events.push('returned');
 let settled=false;p.then(()=>{settled=true;events.push('settled');});
 const settledBeforeAwait=settled;const first=await p;const second=await wrapped();
 const sentinel={tag:'throw-before-promise'};let caughtSame=false,returned=false;
 try{load(()=>{throw sentinel;})();returned=true;}catch(e){caughtSame=e===sentinel;}
 process.stdout.write(JSON.stringify({events,settledBeforeAwait,firstIdentity:first===Constructor,secondIdentity:second===Constructor,caughtSame,returned}));
})().catch(e=>{console.error(e);process.exitCode=1;});
