const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/apexomni/apexomni.utils.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const nodes=tree.statements.filter(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='dropPrefix');
assert.equal(nodes.length,1,'APEX-PREFIX actual declaration selector');
let declaration=nodes[0].getText(tree);
if(process.env.SAFE_CEX_APEX_PREFIX_CONTROL){
 assert.equal(process.env.SAFE_CEX_APEX_PREFIX_CONTROL,'substitute','APEX-PREFIX named control');
 const before="return hash.replace(/^0x/, '');";
 assert.equal(declaration.split(before).length,2,'APEX-PREFIX exact source selector');
 declaration=declaration.replace(before,"return hash.replace(/^0x/, '').replaceAll(String.fromCharCode(0xd800),String.fromCharCode(0xd801));");
}
const api=new vm.Script(ts.transpileModule(declaration,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'\ndropPrefix').runInNewContext({exports:{}});
const cases=[
 ['high',String.raw`"0x\ud800"`],['low',String.raw`"0x\udc00"`],
 ['high-tail',String.raw`"0x\ud800A"`],['low-prefix',String.raw`"0xA\udc00"`],
 ['pair',String.raw`"0x\ud83d\ude00"`],['scalar',String.raw`"0xé"`],
 ['replacement',String.raw`"0x\ufffd"`],['ascii-escape',String.raw`"0x\\ud800"`],
 ['empty','"0x"'],['uppercase','"0X123"'],['leading','" 0x123"'],['interior','"a0x123"'],
];
const units=value=>Array.from({length:value.length},(_,index)=>value.charCodeAt(index));
const rows=cases.map(([label,raw])=>{
 const input=JSON.parse(raw),output=api(input);
 const observation={inputType:typeof input,inputUnits:units(input),outputType:typeof output,outputUnits:units(output)};
 return {label,raw,...observation,outputWire:JSON.stringify(output)};
});
assert.equal(rows.length,12,'APEX-PREFIX independent source floor');
console.log(JSON.stringify({rows,header:{node:process.version,arch:process.arch}}));
