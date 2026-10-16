const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const header = {arch:process.arch, node:process.version, nodeMajor:Number(process.versions.node.split('.')[0]), v8:process.versions.v8};
const file=ts.createSourceFile('exchange.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),'utf8'),99,true);
const names=['isPositiveFinitePrice','normalizePositiveFraction','roundPriceToStep'];
const selected=file.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.length===1&&names.includes(n.declarationList.declarations[0].name.getText(file)));
assert.equal(selected.length,3,'source_helper_census');
const cls=file.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const methods=cls.members.filter(n=>n.name?.getText(file)==='convertIntervalToTradingViewResolution');
assert.equal(methods.length,1,'source_interval_census');
// Execute the actual method body in an otherwise empty class: this method has
// no instance dependencies. No reimplemented arithmetic or switch in the oracle.
const safeMath=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'),'utf8');
const mathModule={exports:{}};
vm.runInNewContext(ts.transpileModule(safeMath,{compilerOptions:{module:1,target:9}}).outputText,{module:mathModule,exports:mathModule.exports,require:n=>{
 // Import is unused by adjust. Refuse an accidental call rather than fake its behavior.
 assert.equal(n,'@tealstreet/math/src/isNotNumber');return {isNotNumber(){throw Error('unexpected safe-math dependency');}};
}});
const moduleValue={exports:{}};
const code=selected.map(n=>n.getText(file)).join('\n')+`\nclass IntervalHarness { ${methods[0].getText(file)} }\nexport { ${names.join(',')}, IntervalHarness };`;
vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:1,target:9}}).outputText,{module:moduleValue,exports:moduleValue.exports,adjust:mathModule.exports.adjust});
const actual=moduleValue.exports, interval=new actual.IntervalHarness();
function bits(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');}
function observation(value){return value===undefined?{type:'undefined'}:typeof value==='number'?{type:'number',bits:bits(value)}:{type:typeof value,value};}
function capture(fn){try{return {ok:true,value:observation(fn())};}catch(e){return {ok:false,name:e.name,message:e.message};}}
const values=[undefined,null,false,true,0,-0,Number.MIN_VALUE,-Number.MIN_VALUE,0.01,0.9999999999999999,1,1.0000000000000002,2,100,1e308,Infinity,-Infinity,NaN,'',' ','0.5','2','1e400','-0','NaN','0x10','0b11',[],[2],[1,2],{}, {toString:null}, {valueOf:2}];
const scalar=values.map((v,i)=>({label:`scalar-${i}`,raw:v===undefined?null:typeof v==='number'?(Number.isNaN(v)?null:v===Infinity?'1e400':v===-Infinity?'-1e400':Object.is(v,-0)?'-0':JSON.stringify(v)):JSON.stringify(v),numberBits:typeof v==='number'?bits(v):null,positive:actual.isPositiveFinitePrice(v),fraction:capture(()=>actual.normalizePositiveFraction(v))}));
const prices=[-Infinity,-1e308,-101.25,-0.25,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,0.1,0.10000000000000002,1,1.0000000000000002,101.25,1e308,Infinity,NaN];
const steps=[-1,-0,0,Number.MIN_VALUE,1e-308,0.000001,0.01,0.1,0.25,1,1e308,Infinity,NaN];
const rounding=[];
for(const price of prices)for(const step of steps)for(const direction of ['up','down'])rounding.push({price:bits(price),step:bits(step),direction,result:bits(actual.roundPriceToStep(price,step,direction))});
const inputs=['','m','1','1m','5h','2d','3w','4M','2y','1W','0001m','0h','-1m','+1m','1.5h',' 1m','1m ','1m\n','1m\r','1m\u2028','١m','１m','1s','1H','1D','constructor','__proto__','😀','9007199254740993m','9007199254740993h','1'+'0'.repeat(308)+'y','9'.repeat(400)+'m'];
const intervals=inputs.map(input=>({input,result:capture(()=>interval.convertIntervalToTradingViewResolution(input))}));
const lone=JSON.parse('"\\ud800"');
const surrogate={raw:'"\\ud800"',fraction:capture(()=>actual.normalizePositiveFraction(lone)),intervalErrorCodeUnit: (()=>{try{interval.convertIntervalToTradingViewResolution(lone);}catch(e){return e.message.charCodeAt('Invalid interval format: '.length);}})()};
process.stdout.write(JSON.stringify({header,scalar,rounding,intervals,surrogate}));
