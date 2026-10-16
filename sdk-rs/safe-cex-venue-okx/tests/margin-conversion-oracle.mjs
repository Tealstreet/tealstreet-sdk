import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
function members(path,ownerName,names){const ast=ts.createSourceFile(path,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===ownerName);return names.map(name=>{const m=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+name);return m.getText(ast);}).join('\n');}
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
const methods=members(url.pathname,'OKXExchange',['toFiniteNumber','getCtValCcy','convertMarginValueToUsd','convertUsdValueToMarginCurrency','contractsToBase']);
const mathPath=new URL('../../../packages/safe-cex/src/utils/safe-math.ts',import.meta.url).pathname;
const mathAst=ts.createSourceFile(mathPath,fs.readFileSync(mathPath,'utf8'),ts.ScriptTarget.Latest,true);
const math=mathAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['POW10_CACHE','pow10','afterDecimal','multiply'].includes(d.name.getText(mathAst)))||ts.isForStatement(n)).map(n=>n.getText(mathAst)).join('\n');
const code=ts.transpileModule(math+'\nexport class Selected {'+methods+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};let stringTrace,stringFailure;
vm.runInNewContext('(function(module,exports){'+code+'\n})',{String:v=>{stringTrace?.push('coerce');if(stringFailure)throw stringFailure;return String(v);}})(module,module.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));
for(const [k,n] of [['finite',24],['currency',30],['margin',168],['base',66]])assert.equal(input[k].length,n,'independent '+k+' floor');
const decode=b=>b===null?undefined:b==='nan'?NaN:Buffer.from(b,'hex').readDoubleBE();
const pack=n=>{if(Number.isNaN(n))return 'nan';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const result={finite:[],currency:[],margin:[],base:[]};
function outcome(fn,error){try{return {bits:pack(fn())};}catch(e){assert.equal(e,error,'original error identity');return {error:true};}}
function call(trace,name,fail,error,value){trace.push(name);if(name===fail)throw error;return value;}
for(const c of input.finite){const r=new module.exports.Selected(),error=new TypeError('conversion failure');
 const values={undefined:undefined,null:null,numberZero:-0,stringZero:'-0',prefix:'12.5junk',invalid:'no',infinity:Infinity,nan:NaN,object:{toString(){return '-3.75x';}},space:'\ufeff\u00a02e2end',surrogate:'17\ud8009',symbol:Symbol('3')};
 const v=c.fail?{[Symbol.toPrimitive](){throw error;}}:values[c.kind];stringTrace=undefined;result.finite.push(outcome(()=>r.toFiniteNumber(v),error));
}
for(const c of input.currency){const r=new module.exports.Selected(),trace=[],error=new TypeError('conversion failure');
 const values={missing:undefined,null:null,false:false,zero:0,nan:NaN,empty:'',lower:'btc',object:{toString(){return 'eth';}},expansion:'ß',surrogate:'\ud800btc'};
 Object.defineProperty(r,'unused',{value:0});
 const market={get info(){trace.push('read');if(c.fail==='read')throw error;return {ctValCcy:values[c.kind]};}};
 stringTrace=trace;stringFailure=c.fail==='coerce'?error:undefined;let resultValue;
 try{const value=r.getCtValCcy(market);resultValue={units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};}
 catch(e){assert.equal(e,error);resultValue={error:true};}
 result.currency.push({trace,outcome:resultValue});stringTrace=undefined;stringFailure=undefined;
}
for(const c of input.margin){const r=new module.exports.Selected(),trace=[],error=new TypeError('conversion failure'),s=c.scenario;
 r.toFiniteNumber=()=>call(trace,'finite',c.fail,error,decode(c.amount));r.resolveReferencePrice=()=>call(trace,'price',c.fail,error,decode(c.price));
 const currency=s===1?undefined:s===2?'':{toUpperCase(){return call(trace,'currency',c.fail,error,(s===13?'ß':'btc').toUpperCase());}};
 const market=s===0?undefined:{get isInverse(){return call(trace,'inverse',c.fail,error,s!==3);},get base(){return {toUpperCase(){return call(trace,'base',c.fail,error,(s===5?'eth':s===13?'ß':'btc').toUpperCase());}};}};
 const o=outcome(()=>r[c.direction==='usd'?'convertMarginValueToUsd':'convertUsdValueToMarginCurrency'](6,currency,market),error);result.margin.push({trace,outcome:o});
}
for(const c of input.base){const r=new module.exports.Selected(),trace=[],error=new TypeError('conversion failure');
 r.getCtValCcy=()=>call(trace,'ct',c.fail,error,'USD');r.resolveReferencePrice=()=>call(trace,'price',c.fail,error,decode(c.price));
 const market={get contractSize(){return call(trace,'size',c.fail,error,decode(c.size));},get isInverse(){return call(trace,'inverse',c.fail,error,c.inverse);},get quote(){return {toUpperCase(){return call(trace,'quote',c.fail,error,c.matches?'usd'.toUpperCase():'btc'.toUpperCase());}};}};
 result.base.push({trace,outcome:outcome(()=>r.contractsToBase(decode(c.contracts),market),error)});
}
process.stdout.write(JSON.stringify(result));
