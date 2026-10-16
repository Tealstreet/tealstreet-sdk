// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url), ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const text=fs.readFileSync(new URL('../../../packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.ts',import.meta.url),'utf8');
const ast=ts.createSourceFile('chaser.ts',text,ts.ScriptTarget.Latest,true);
const declaration=ast.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(ast)==='placementFilled'));
if(!declaration)throw new Error('placementFilled source disappeared');
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(declaration.getText(ast).replace(/^export /,'')+'\nglobalThis.readFill=placementFilled;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const encode=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':n;
const rows=[];
for(const filled of [-Infinity,-3,-0,0,0.125,1e200,Infinity,NaN])for(const placement of [undefined,-Infinity,-1,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,0.125,1e200,Infinity,NaN]){
 const order={filled,extra:{filledInPlacementUnits:placement}};
 rows.push({filled:encode(filled),...(placement===undefined?{}:{placement:encode(placement)}),result:encode(context.readFill(order))});
}
for(const filled of [-0,1,NaN])rows.push({filled:encode(filled),noExtra:true,result:encode(context.readFill({filled}))});
process.stdout.write(JSON.stringify(rows));
