const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {isNativeError} = require('node:util').types;
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/utils/signatureRejection.ts'),'utf8');
const moduleObject = {exports:{}};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{module:moduleObject,exports:moduleObject.exports});
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.api.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=[];
function walk(node){if(ts.isVariableDeclaration(node)&&node.name.getText(ast)==='isBingxSignatureRejection')declarations.push(node);ts.forEachChild(node,walk);}
walk(ast);
assert.equal(declarations.length,1,'SIGNATURE-TYPEERROR exact maintained BingX predicate');
const predicateModule={exports:{}};
vm.runInNewContext(ts.transpileModule(`module.exports = ${declarations[0].initializer.getText(ast)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{module:predicateModule});
let count=0;
for(const predicate of [moduleObject.exports.isBinanceStyleSignatureRejection,predicateModule.exports]){
 for(const data of [{code:{toString:0}},{code:0,msg:{toString:0}}]){
  for(const rejected of [false,true]){
   const config={},response={config,data};let resends=0, failure;
   try{
    if(rejected)moduleObject.exports.signatureRejectedResponse(predicate)({config,response});
    else moduleObject.exports.resendOnSignatureRejection({request(){resends++;throw Error('unexpected resend');}},response,predicate);
   }catch(error){failure=error;}
   assert(isNativeError(failure),'SIGNATURE-TYPEERROR actual source throws native Error');
   assert.equal(failure.name,'TypeError');
   assert.equal(failure.message,'Cannot convert object to primitive value');
   assert.equal(resends,0,'SIGNATURE-TYPEERROR coercion failure is not a signature rejection');
   assert.equal(Object.hasOwn(config,moduleObject.exports.SIGNATURE_RETRY_CONFIG_KEY),false);
   count++;
  }
 }
}
assert.equal(count,8);
console.log('SIGNATURE-TYPEERROR actual shared/BingX source: 8 predicate failures retain TypeError and do not resend PASS');
