// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
// Field-identity supplement to the original cancellation/rounding fixture; no historical independence claim.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const side=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='PositionSide');
const path='webworker/postProcessors/DisplayedMarginPostprocessor.ts';
const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='DisplayedMarginPostprocessor');
const method=cls.members.find(n=>ts.isMethodDeclaration(n)&&n.name.getText()==='process');
if(!side||!method)throw Error('MARGIN-IDENTITY actual source missing');
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(side.getText().replace('export ','')+'\nclass Probe {'+method.getText()+'}\nglobalThis.probe=new Probe();',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const rows=[];
for(const [long,short] of [[10,3],[7,11],[1.25,2.5]]){
 const positions=[{side:'long',notional:-long},{side:'short',notional:short}],store={displayedPositions:positions};
 context.probe.process(store);rows.push({positions,expected:store.displayedMargin});
}
process.stdout.write(JSON.stringify({architecture:`${process.platform}/${process.arch}`,nodeMajor:Number(process.versions.node.split('.')[0]),rows}));
