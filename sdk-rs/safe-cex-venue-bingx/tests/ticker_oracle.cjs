const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function compile(source,file) {
 const result=ts.transpileModule(source,{fileName:file,reportDiagnostics:true,
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});
 assert(!result.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error),'transpile '+file);
 return result.outputText;
}
const cache=new Map();
function load(file) {
 if(cache.has(file))return cache.get(file);
 const module={exports:{}};
 const requireSource=spec=>{
  assert.equal(spec,'@tealstreet/math/src/isNotNumber','known safe-math dependency');
  return load(path.join(root,'packages/math/src/isNotNumber.ts'));
 };
 vm.runInThisContext(`(function(require,module,exports){${compile(fs.readFileSync(file,'utf8'),file)}\n})`,{filename:file})(requireSource,module,module.exports);
 cache.set(file,module.exports);return module.exports;
}
const math=load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const classes=source.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text==='BingxExchange');
assert.equal(classes.length,1);
const fields=classes[0].members.filter(m=>ts.isPropertyDeclaration(m)&&m.name?.getText(source)==='deriveTickerBidAsk');
assert.equal(fields.length,1,'ticker helper census');
assert(ts.isArrowFunction(fields[0].initializer));
const derive=vm.runInNewContext(compile(`(${fields[0].initializer.getText(source)})`,file),{add:math.add,subtract:math.subtract},{filename:file});
function bits(value){if(Number.isNaN(value))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex');}
const lasts=[NaN,Infinity,-Infinity,0,-0,-1,0.005,0.1,1.23,12.01,9007199254740992,5e-324,1e308,Number.MAX_VALUE];
const precisions=[null,{price:0.01,amount:1},{price:0.1,amount:0.001},{price:0.25,amount:2},{price:1,amount:0.01},
 {price:0,amount:1},{price:-0,amount:1},{price:-1,amount:0.1},{price:NaN,amount:1},
 {price:Infinity,amount:1},{price:5e-324,amount:0.1},{price:Number.MAX_VALUE,amount:1}];
const cases=[];
for(const precision of precisions)for(const last of lasts){
 const actual=derive(precision?{precision}:{},last);
 cases.push({last:bits(last),precision:precision?{price:bits(precision.price),amount:bits(precision.amount)}:null,bid:bits(actual.bid),ask:bits(actual.ask)});
}
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases}));
