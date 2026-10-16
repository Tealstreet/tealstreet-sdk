// Independent declaration resolver and actual strict compiler/JSON.parse oracle.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),directory=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid');
const config=[['hyperliquidspot.types.ts','spot',['AllMids','UserRateLimit','OrderStatus']],['hyperliquid.fast-asset-ctxs.ts','fast',['HyperliquidFastAssetCtx','HyperliquidFastAssetCtxs']],['hyperliquid.cross-cache.ts','cache',['PrePlacementEntry','PendingOrderEntry']],['hyperliquid.agent.ts','agent',['AgentWalletCreationResult']]];
const roots=[],definitions=new Map();
for(const [file,module,names]of config){const source=ts.createSourceFile(path.join(directory,file),fs.readFileSync(path.join(directory,file),'utf8'),ts.ScriptTarget.Latest,true);require('./interface-heritage.cjs')(ts,source);for(const name of names){const node=source.statements.find(n=>(ts.isInterfaceDeclaration(n)||ts.isTypeAliasDeclaration(n))&&n.name.text===name);assert(node);definitions.set(name,{source,node,module});roots.push(name);}}
assert.equal(roots.length,8,'independent eight roots');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function anyNative(v){if(v===null)return ['null'];if(typeof v==='boolean')return ['bool',v];if(typeof v==='number')return ['number',bits(v)];if(typeof v==='string')return ['string',units(v)];if(Array.isArray(v))return ['array',v.map(anyNative)];return ['object',Object.keys(v).map(k=>[units(k),anyNative(v[k])])];}
function shape(name){const {node}=definitions.get(name);return node.members??node.type;}
function projection(node,value){
 if(ts.isTypeReferenceNode(node)){const name=node.typeName.getText();if(name==='OrderType')return value;if(name==='Record')return ['object',Object.keys(value).map(k=>[units(k),projection(shape(node.typeArguments[1].getText()),value[k])])];return projection(shape(name),value);}
 const members=Array.isArray(node)?node:ts.isTypeLiteralNode(node)?node.members:null;
 if(members){if(members.length===1&&ts.isIndexSignatureDeclaration(members[0])){const type=members[0].type;if(type.kind===ts.SyntaxKind.AnyKeyword)return anyNative(value);return ['object',Object.keys(value).map(k=>[units(k),projection(type,value[k])])];}
 return Object.fromEntries(members.map(m=>[m.name.text,Object.hasOwn(value,m.name.text)?projection(m.type,value[m.name.text]):{absent:true}]));}
 if(node.kind===ts.SyntaxKind.StringKeyword)return units(value);if(node.kind===ts.SyntaxKind.NumberKeyword)return bits(value);if(node.kind===ts.SyntaxKind.BooleanKeyword)return value;throw Error('unsupported native AST '+ts.SyntaxKind[node.kind]);
}
const raw=s=>({raw:s});
function generate(node,mode,context){
 if(ts.isTypeReferenceNode(node)){const name=node.typeName.getText();if(name==='OrderType')return ['market','limit','stop_market','take_profit_market'][mode];if(name==='Record')return dictionary(node.typeArguments[1],mode,context);return generate(shape(name),mode,context);}
 const members=Array.isArray(node)?node:ts.isTypeLiteralNode(node)?node.members:null;
 if(members){if(members.length===1&&ts.isIndexSignatureDeclaration(members[0]))return dictionary(members[0].type,mode,context);return Object.fromEntries(members.filter(m=>!m.questionToken||mode!==1).map(m=>[m.name.text,generate(m.type,mode,context)]));}
 const i=context.index++;
 if(node.kind===ts.SyntaxKind.StringKeyword)return [`v:${context.name}:${i}:"\\`, '',`\ud800${i}😀\udfff`,`é雪😀\0:${i}`][mode];
 if(node.kind===ts.SyntaxKind.NumberKeyword)return raw([['17.5','91.25','-0'][i%3],'-0',['1e400','-1e400','9007199254740993'][i%3],['1e-9999','-1e-9999','1.2345678901234567'][i%3]][mode]);
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return mode!==1;
 if(node.kind===ts.SyntaxKind.AnyKeyword)return {nested:[raw('-0'),raw('1e400'),`\ud800${i}`],flag:false};
 throw Error('unsupported generated AST '+ts.SyntaxKind[node.kind]);
}
function dictionary(type,mode,context){if(mode===1)return {};return Object.fromEntries(['10','2','01','__proto__','\ud800','z','4294967295'].map(k=>[k,generate(type,mode,context)]));}
function render(v){if(v&&Object.hasOwn(v,'raw'))return v.raw;if(Array.isArray(v))return '['+v.map(render).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).map(([k,v])=>JSON.stringify(k)+':'+render(v)).join(',')+'}';return JSON.stringify(v);}
// Preserve native binary64 values when making a strict checker literal; narrow
// actual source enum values through its declared enum, never an unchecked cast.
const enumValues={market:'Market',limit:'Limit',conditional:'Conditional',stop_market:'StopLoss',take_profit_market:'TakeProfit',trailing_stop_market:'TrailingStopLoss'};
function checkerLiteral(v,key){if(key==='type'&&typeof v==='string'&&enumValues[v])return 'OrderType.'+enumValues[v];if(typeof v==='number'){if(v===Infinity)return '1e400';if(v===-Infinity)return '-1e400';if(Object.is(v,-0))return '-0';return String(v);}if(Array.isArray(v))return '['+v.map(v=>checkerLiteral(v)).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).map(([k,v])=>JSON.stringify(k)+':'+checkerLiteral(v,k)).join(',')+'}';return JSON.stringify(v);}
const cases=[];
function add(name,input,expectedAccepted,mode){cases.push({name,input,mode,expectedAccepted,...expectedAccepted?{native:projection(shape(name),JSON.parse(input))}:{}});}
for(const name of roots){const node=shape(name);for(let mode=0;mode<4;mode++)add(name,render(generate(node,mode,{name,index:0})),true,mode);
 let wrong='17';if(name==='AllMids')wrong='{"x":false}';else if(name==='HyperliquidFastAssetCtxs')wrong='{"x":{"markPx":false}}';else if(!['UserRateLimit','OrderStatus'].includes(name)){const v=generate(node,0,{name,index:0});const members=Array.isArray(node)?node:node.members;const first=members[0];v[first.name.text]=first.type.kind===ts.SyntaxKind.BooleanKeyword?'wrong':false;wrong=render(v);}
 for(const input of ['null','false',name==='UserRateLimit'||name==='OrderStatus'?'"wrong"':'[]',wrong])add(name,input,false,'reject');
}
assert.equal(cases.length,64,'independent eight roots times eight case floor');
for(const name of ['UserRateLimit','OrderStatus'])for(const input of ['[]','[-0,1e400,"\\ud800",{"__proto__":false}]'])add(name,input,true,'native array alternative');
add('AllMids','{"x":false,"x":"\\ud800","2":"two","10":"ten","01":"leading","__proto__":"own"}',true,'last duplicate/order');
add('UserRateLimit','{"x":false,"x":{"\\ud800":-0},"2":1e400,"10":"ten","__proto__":["\\udfff"]}',true,'last duplicate/order');
add('HyperliquidFastAssetCtxs','{"x":false,"x":{"markPx":"\\ud800","midPx":"\\udfff"}}',true,'last duplicate before typed decode');
assert.equal(cases.length,71,'independent seven targeted witnesses');
let probe=config.map(([file,module])=>`import * as ${module} from './${file.replace(/\.ts$/,'')}';`).join('\n')+"\nimport {OrderType} from '../../types';\n";
for(const [i,row]of cases.entries()){
 // as const would make array alternatives readonly and change assignability.
 // The generated positive records use source enum refs; dictionary arrays use
 // ordinary mutable literals and structurally checked non-fresh variables.
 const value=row.expectedAccepted?checkerLiteral(JSON.parse(row.input)):row.input;
 if(!row.expectedAccepted)probe+=`const case_${i}:${definitions.get(row.name).module}.${row.name}=${value};\n`;
 else probe+=`const parsed_${i}${value==='[]'?':never[]':''}=${value}; const case_${i}:${definitions.get(row.name).module}.${row.name}=parsed_${i};\n`;
}
const filename=path.join(directory,'__json_models_probe__.ts'),options={strict:true,noEmit:true,skipLibCheck:true,target:ts.ScriptTarget.ES2022};
const host=ts.createCompilerHost(options),read=host.readFile.bind(host),exists=host.fileExists.bind(host);host.readFile=f=>f===filename?probe:read(f);host.fileExists=f=>f===filename||exists(f);
const p=ts.createProgram([filename],options,host),file=p.getSourceFile(filename);assert.equal(p.getSyntacticDiagnostics(file).length,0);
const rejected=new Set();for(const d of p.getSemanticDiagnostics(file)){assert(d.start!==undefined);const st=file.statements.find(n=>n.getStart(file)<=d.start&&d.start<n.end);assert(st&&ts.isVariableStatement(st));const match=/case_(\d+)/.exec(st.declarationList.declarations[0].name.text);assert(match,ts.flattenDiagnosticMessageText(d.messageText,' '));assert([2322,2740,2741,2739,2559,2353].includes(d.code),'unexpected diagnostic '+d.code);rejected.add(Number(match[1]));}
for(const [i,row]of cases.entries()){row.accepted=!rejected.has(i);assert.equal(row.accepted,row.expectedAccepted,`strict JSON model ${row.name} case${i}: ${row.input}`);}
console.log(JSON.stringify({roots,cases,iterationStop:70,scope:'eight raw JSON boundary roots; own properties/array alternatives, UTF16 units, binary64 bits; no prototypes/live alias identity'}));
