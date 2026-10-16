const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
function execute(code,globals={}){const module={exports:{}};const out=ts.transpileModule(code,{reportDiagnostics:true,compilerOptions:{module:1,target:9}});assert.equal(out.diagnostics.length,0,'maps_transpile_diagnostics');vm.runInNewContext(out.outputText,{module,exports:module.exports,...globals});return module.exports;}
const source=ts.createSourceFile('exchange.ts',read('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),99,true);assert.equal(source.parseDiagnostics.length,0);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const names=['reverseMapOrderType','mapOrderStatus'];
const members=names.map(name=>{const nodes=cls.members.filter(n=>n.name?.getText(source)===name);assert.equal(nodes.length,1,'maps_source_identity');return nodes[0].getText(source);});
const shared=ts.createSourceFile('types.ts',read('packages/safe-cex/src/types.ts'),99,true);
const enums=execute(shared.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide','OrderStatus'].includes(n.name.text)).map(n=>n.getText(shared)).join('\n'));
const {ORDER_TYPE}=execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'),{require:()=>enums});
const {Harness}=execute(`export class Harness { ${members.join('\n')} }`,{...enums,ORDER_TYPE});
const h=new Harness();
const keys=['NEW','OPEN','PENDING','UNTRIGGERED','FILLED','CANCELED','REJECTED','LIMIT','MARKET','STOP_LIMIT','STOP_MARKET','STOP_LOSS_LIMIT','STOP_LOSS_MARKET','TAKE_PROFIT_LIMIT','TAKE_PROFIT_MARKET','constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString','','new','unknown','é😀','STOP_MARKET '];
function observe(value){return {kind:typeof value,text:String(value),truthy:!!value,arity:typeof value==='function'?value.length:null,wire:JSON.stringify({value})};}
const cases=keys.map(key=>({key,status:observe(h.mapOrderStatus(key)),types:[undefined,true,false].map(reduce=>({reduce:reduce??null,out:observe(h.reverseMapOrderType(key,reduce))}))}));
const lone=String.fromCharCode(0xd800);
process.stdout.write(JSON.stringify({header:{arch:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),node:process.version},cases,surrogate:{raw:JSON.stringify(lone),status:observe(h.mapOrderStatus(lone)),type:observe(h.reverseMapOrderType(lone))}}));
