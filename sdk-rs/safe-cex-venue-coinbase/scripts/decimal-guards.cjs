const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const sourcePath=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.exchange.ts');
const sourceText=fs.readFileSync(sourcePath,'utf8');
const source=ts.createSourceFile(sourcePath,sourceText,ts.ScriptTarget.Latest,true,ts.ScriptKind.TS);
function declaration(name){
 return source.statements.filter(ts.isVariableStatement).flatMap(node=>node.declarationList.declarations)
  .find(node=>ts.isIdentifier(node.name)&&node.name.text===name);
}
function evaluate(name,context={}){
 const node=declaration(name)?.initializer;
 if(!node)throw Error('missing '+name);
 const code=ts.transpileModule(node.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText
  .replace(/^"use strict";\s*/,'').replace(/;\s*$/,'');
 return vm.runInNewContext('('+code+')',context);
}
const unsigned=evaluate('COINBASE_UNSIGNED_DECIMAL');
const zero=evaluate('COINBASE_ZERO_DECIMAL');
const context={COINBASE_UNSIGNED_DECIMAL:unsigned,COINBASE_ZERO_DECIMAL:zero};
const isUnsigned=evaluate('isCoinbaseUnsignedDecimal',context);
const isZero=evaluate('isCoinbaseZeroDecimal',context);
const cases=[
 {kind:'undefined'}, {value:null}, {value:false}, {value:0}, {value:1}, {value:[]}, {value:{}},
 ...['','0','00','1','01','9','10','0.0','0.000','00.0','1.0','1.','1.00','.1','-0','+0','NaN','Infinity','1e3','１２','١','1\n',' 0','\ufeff0','0x10','0001','000.','0.00.0'].map(value=>({value})),
];
const values=cases.map(item=>item.kind==='undefined'?undefined:item.value);
process.stdout.write(JSON.stringify({
 unsignedPattern:unsigned.toString(),zeroPattern:zero.toString(),
 results:values.map(value=>[isUnsigned(value),isZero(value)]),
}));
