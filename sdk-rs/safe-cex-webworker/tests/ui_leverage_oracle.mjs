// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
const types=ast('types.ts');
const enums=['PositionMode','PositionSide'].map(name=>{
 const node=types.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text===name);
 if(!node)throw Error(`missing enum ${name}`);return node.getText(types);
}).join('\n');
const handler=ast('webworker/handlers/UiStateHandler.ts');
const source=handler.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(handler)).join('\n');
const context=vm.createContext({BaseHandler:class{constructor(exchange){this._exchange=exchange;}}});
vm.runInContext(ts.transpileModule((enums+'\n'+source).replace(/\bexport /g,'')+'\nglobalThis.Handler=UiStateHandler;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){if(Number.isNaN(n))return 'NaN';bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
const values=[0,-0,1,2,NaN,Infinity,-Infinity],rows=[];
for(const mode of [undefined,'oneway','hedged'])for(const side of ['long','short'])for(const old of values)for(const buy of values)for(const sell of values){
 const positions=[{id:'other',symbol:'OTHER',side,positionMode:mode,leverage:old},{id:'first',symbol:'BTC',side,positionMode:mode,leverage:old},{id:'second',symbol:'BTC',side:side==='long'?'short':'long',positionMode:mode,leverage:old}];
 const calls=[];
 // Capture this helper's production Store call boundary; Store behavior is tested separately in Rust.
 const store={positions,updatePositions(updates){calls.push(updates.map(([target,patch])=>[target,{leverage:bits(patch.leverage)}]));}};
 new context.Handler({store}).updatePositionsWithLeverage('BTC',{buyLeverage:buy,sellLeverage:sell});
 rows.push({mode,side,old:bits(old),buy:bits(buy),sell:bits(sell),calls});
}
const emptyCalls=[];
new context.Handler({store:{positions:[],updatePositions(v){emptyCalls.push(v);}}}).updatePositionsWithLeverage('BTC',{buyLeverage:1,sellLeverage:2});
process.stdout.write(JSON.stringify({rows,emptyCalls}));
