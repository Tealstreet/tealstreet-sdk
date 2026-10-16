const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const ast=ts.createSourceFile('woo.exchange.ts',fs.readFileSync(path.join(root,'exchanges/woo/woo.exchange.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const member=cls.members.find(n=>n.name?.getText(ast)==='isAlgoOrder');
assert.ok(member,'POLICY actual source member');
const types=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const declaration=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderType');
assert.ok(declaration,'POLICY actual enum');
const context={exports:{}};
vm.runInNewContext(ts.transpileModule(declaration.getText(types),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,context);
context.OrderType=context.exports.OrderType;
vm.runInNewContext(ts.transpileModule(`class Witness {${member.getText(ast)}};globalThis.Witness=Witness;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const subject=new context.Witness();
const expected=[['market',false],['limit',false],['conditional',false],['stop_market',true],['take_profit_market',true],['trailing_stop_market',true]];
assert.deepEqual(Object.values(context.OrderType),expected.map(([value])=>value),'POLICY independent enum inventory');
const cases=[];
for(const [input,result] of expected){
 const actual=subject.isAlgoOrder(input);
 assert.equal(typeof actual,'boolean','POLICY native source boolean');
 assert.equal(actual,result,'POLICY source classification');
 cases.push({input,result:actual});
}
for(const input of [null,false,0,'','STOP_LOSS','stop_market ',[],{},['stop_market']]){
 const result=subject.isAlgoOrder(input);
 assert.equal(result,false,'POLICY source strict equality');cases.push({input,result});
}
assert.equal(subject.isAlgoOrder(undefined),false,'POLICY missing input');
cases.push({missing:true,result:false});
assert.equal(cases.length,16,'POLICY independent source total');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,scope:'actual predicate and OrderType declaration'},cases}));
