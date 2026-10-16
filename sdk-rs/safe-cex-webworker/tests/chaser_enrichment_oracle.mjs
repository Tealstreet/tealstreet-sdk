// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts',import.meta.url);
const text=fs.readFileSync(path,'utf8');
const ast=ts.createSourceFile(path.pathname,text,ts.ScriptTarget.Latest,true);
const source=ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
const context=vm.createContext({BasePostprocessor:class {}});
vm.runInContext(ts.transpileModule(source+'\nglobalThis.Processor=ChaserOrderEnrichmentPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const base={id:'first',symbol:'BTC',size:2,side:'buy',maxAttempts:7,attempts:3,isStopped:false,stopReason:''};
const ids=[...Object.getOwnPropertyNames(Object.prototype),...Object.keys(base),'currentOrderIds','reduceOnly','origin','ordinary','unowned',''];
const cases=[[],[{...base,currentOrderIds:[]}],[{...base,currentOrderIds:['ordinary']}],[{...base,currentOrderIds:['ordinary','constructor']},{...base,id:'last',attempts:5,currentOrderIds:['ordinary']}],[{...base,currentOrderIds:['__proto__']}],[{...base,currentOrderIds:['__proto__'],reduceOnly:true,origin:'web',isStopped:true,stopReason:'stop'}],[{...base,currentOrderIds:['__proto__'],id:'',symbol:'',size:0,maxAttempts:0,attempts:0,reduceOnly:false}],[{...base,currentOrderIds:['__proto__','id','toString']}],[{...base,currentOrderIds:['__proto__']},{...base,id:'last',attempts:9,currentOrderIds:['__proto__']}],[{...base,currentOrderIds:ids}]];
cases.push([{...base,currentOrderIds:['__proto__'],reduceOnly:true,isStopped:false}],[{...base,currentOrderIds:['__proto__'],reduceOnly:false,isStopped:true}]);
const rows=[];
for(const activeChasers of cases)for(const oldExtra of [undefined,{remainingNotional:42,chaserId:'stale',chaserAttempts:97,chaserMaxAttempts:99}]){
 const orders=ids.map(id=>({id,symbol:'BTC',status:'open',type:'limit',side:'buy',price:1,amount:2,filled:4,remaining:3,reduceOnly:false,lastUpdate:5,...(oldExtra?{extra:{...oldExtra}}:{})}));
 const input={activeChasers,orders};const store=structuredClone(input);
 new context.Processor().process(store);
 rows.push({input,expected:store.orders});
}
process.stdout.write(JSON.stringify(rows));
