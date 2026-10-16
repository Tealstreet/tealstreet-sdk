'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.builder.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const declaration = tree.statements.find(s => ts.isClassDeclaration(s) && s.name?.text === 'GrvtBuilderUnauthorizedError');
assert(declaration, 'actual builder error class');
let text = declaration.getText(tree);
if (process.env.GRVT_BUILDER_ERROR_MUTATION) {
  assert.equal(process.env.GRVT_BUILDER_ERROR_MUTATION, 'nullish');
  assert(text.includes('opts.authorizedRaw ??'));
  text = text.replace('opts.authorizedRaw ??', 'opts.authorizedRaw ||');
}
const output = ts.transpileModule(text, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(output.diagnostics.length, 0);
const exportsObject = {};
vm.runInNewContext(output.outputText, {exports:exportsObject}, {filename:file});
const Constructor = exportsObject.GrvtBuilderUnauthorizedError;
const {bits} = require('../../safe-cex-num/tests/support/number-bits.cjs');
const numbers = [0,-0,NaN,Infinity,-Infinity,300,299.5,-0.1,Number.MIN_VALUE,-Number.MIN_VALUE,Number.MAX_VALUE,1e-7,1e-6,1e20,1e21,9007199254740992];
const rows=[];
for (const reason of ['unauthorized','cap-too-low']) for (const requiredRaw of numbers) for (const authorizedRaw of [undefined,...numbers]) for (const message of [undefined,'','override 😀']) {
  const error = new Constructor({reason,requiredRaw,authorizedRaw,message});
  rows.push({reason,required:bits(requiredRaw),authorized:authorizedRaw===undefined ? null : bits(authorizedRaw),inputMessage:message,name:error.name,code:error.code,message:error.message,string:String(error),requiredOut:bits(error.requiredRaw),authorizedOut:error.authorizedRaw===undefined ? null : bits(error.authorizedRaw),reasonOut:error.reason,ownAuthorized:Object.hasOwn(error,'authorizedRaw'),hasStack:typeof error.stack==='string',errorPrototype:Object.getPrototypeOf(Constructor.prototype).constructor.name});
}
const refused=['\uD800','\uDC00','cap\uD800'].map(message=>{
 const error=new Constructor({reason:'cap-too-low',requiredRaw:300,message});
 return{wire:JSON.stringify(message),units:error.message.split('').map(c=>c.charCodeAt(0))};
});
process.stdout.write(JSON.stringify({rows,refused}));
