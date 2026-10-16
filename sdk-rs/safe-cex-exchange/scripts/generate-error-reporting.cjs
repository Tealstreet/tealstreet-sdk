// Evaluates source methods with inert telemetry dependencies; no network or venue adapters.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
const load=(relative,imports={})=>{const exports={};const source=fs.readFileSync(path.join(root,'packages/safe-cex/src',relative),'utf8');const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;new Function('exports','require',compiled)(exports,name=>imports[name]||{});return exports;};
const failures=load('utils/httpFailure.ts');const contexts=load('utils/httpFailureContext.ts',{'./httpFailure':failures});
const baseSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/base.ts'),'utf8');
const base=ts.createSourceFile('base.ts',baseSource,ts.ScriptTarget.Latest,true);
const baseClass=base.statements.find(node=>ts.isClassDeclaration(node)&&node.name.text==='BaseExchange');
const methods=['noteHttpResponse','emitAndLogError'].map(name=>baseClass.members.find(node=>node.name&&node.name.getText(base)===name).getText(base)).join('\n');
const axiosSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/axiosErrorEnhancer.ts'),'utf8');const axiosAst=ts.createSourceFile('enhancer.ts',axiosSource,ts.ScriptTarget.Latest,true);const formatter=axiosAst.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name.text==='formatErrorForSentry').getText(axiosAst);
const compiled=ts.transpileModule(`export class Oracle {${methods}}\n${formatter}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const identity=process.argv.includes('--identity');
const cases=[];
for(const worker of [false,true]) for(const [testnet,publicDataOnly] of (identity?[[false,true],[true,false]]:[[false,false]])){
 const events=[],reports=[],exports={};let now=10000;
 new Function('exports','recordHttpFailure','readHttpFailureContext','reportException','Date','console','self','window',compiled)(exports,contexts.recordHttpFailure,contexts.readHttpFailureContext,(error,data)=>reports.push({error,data}),{now:()=>now},{error(){},warn(){},trace(){}},worker?{}:undefined,worker?undefined:{});
 const instance=new exports.Oracle();Object.assign(instance,{name:'bybit',options:{testnet,publicDataOnly},isDisposed:false,emitter:{emit(...args){events.push(args);}}});
 const emit=(input,msg,extra)=>{events.length=0;reports.length=0;const error=input&&typeof input==='object'?Object.assign(new Error(input.message),input):input;instance.emitAndLogError(error,msg,extra);cases.push({worker,...(identity?{options:{testnet,publicDataOnly}}:{}),input:input??null,msg:msg??null,extra:extra??null,events:[...events],reports:[...reports]});};
 emit(null);
 instance.noteHttpResponse({status:429,statusText:'Too Many Requests'},'/private?nonce=secret');now=10550;
 emit('order warning',undefined,{sentryTags:{exchange:'override'},sentryExtra:{safe:'detail'}});
 emit('second warning');
 instance.noteHttpResponse({status:403,status_text:'Forbidden'},'/keys?secret=raw');
 emit({message:'causal',context:{statusCode:401,isTimeout:false},enhancedMessage:'enhanced causal',config:{headers:{Authorization:'secret'}}});
 emit('still correlated');
 instance.noteHttpResponse({status:500},'/old');now+=1001;emit('expired');
 instance.noteHttpResponse({status:404},'/missing');instance.noteHttpResponse({status:200},'/ok');emit('cleared');
 emit({message:'dropped',name:'TradeTunnelError',url:'/tunnel',method:'POST',originalError:{message:'socket closed'}},'override',{suppressConsoleLog:true,legacy:'preserved'});
 instance.isDisposed=true;emit('disposed');
}
const text=JSON.stringify({generatedFrom:'packages/safe-cex/src/exchanges/base.ts',cases},(key,value)=>value instanceof Error?{name:value.name,message:value.message}:value,2)+'\n';
const target=path.resolve(__dirname,'../tests/error_reporting_oracle.json');
if(identity)process.stdout.write(text);
else if(process.argv.includes('--check')){if(fs.readFileSync(target,'utf8')!==text)throw Error('error reporting oracle drifted');}else fs.writeFileSync(target,text);
