const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts'),'utf8');
const ast=ts.createSourceFile('hyperliquid.exchange.ts',source,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const names=['getOrderBasePrice','shouldUsePositionTpsl'];const members=cls.members.filter(n=>names.includes(n.name?.getText(ast)));assert.equal(members.length,2);
const predicate=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='isFinitePositiveNumber'));assert(predicate);
const out={};vm.runInNewContext(ts.transpileModule(predicate.getText(ast)+'\nclass Subject {'+members.map(n=>n.getText(ast)).join('\n')+'} out.Subject=Subject;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{out,Number,OrderSide:{Buy:'buy'},PositionSide:{Short:'short',Long:'long'}});
if(process.argv.includes('--string-boundary')) {
 const cases=[];
 for(const value of ['BTC','é','😀','\ud800','\udfff']) {
  const s=new out.Subject();s.store={tickersBySymbol:{},positions:[]};
  let message;try{s.getOrderBasePrice({symbol:value,isSpot:true},value)}catch(e){message=e.message}
  const tpsl=s.shouldUsePositionTpsl({amount:0,reduceOnly:false,side:'buy'},{symbol:value},1);
  assert.equal(tpsl,true,'source zero amount precedes symbol lookup');
  assert(message.includes(value),'source diagnostic retains string code units');
  cases.push({value,message,tpsl});
 }
 process.stdout.write(JSON.stringify({captureArchitecture:process.arch,nodeVersion:process.version,cases}));
 process.exit(0);
}
const bits=require('./number-bits.cjs');
const cases=[];
for(const isSpot of [true,false]) for(const values of [[1,2,3,4],[0,2,3,4],[-1,0,3,4],[NaN,Infinity,0,4],[0,0,0,0]]) {
 const s=new out.Subject(),reads=[];const ticker={};for(const [i,key] of ['last','midPrice','mark','index'].entries())Object.defineProperty(ticker,key,{get(){reads.push(key);return values[i]}});
 s.store={tickersBySymbol:{BTC:ticker}};let result,error;try{result=s.getOrderBasePrice({symbol:'BTC',isSpot},'limit')}catch(e){error={name:e.name,message:e.message}}
 cases.push({kind:'price',isSpot,values:values.map(String),reads,...(error?{error}:{bits:bits(result)})});
}
for(const amount of [0,1])for(const reduceOnly of [false,true])for(const size of [0,1,2,NaN,Infinity]) {
 const s=new out.Subject();let reads=0;Object.defineProperty(s,'store',{value:{get positions(){reads++;return [{symbol:'BTC',side:'short',contracts:2},{symbol:'BTC',side:'short',contracts:1}]}}});
 const result=s.shouldUsePositionTpsl({amount,reduceOnly,side:'buy'},{symbol:'BTC'},size);
 cases.push({kind:'tpsl',amount,reduceOnly,size:String(size),reads,result});
}
const output=JSON.stringify({scope:'actual source member bodies; injected store read surface, not Store construction or mutation',cases},null,2)+'\n';
const target=path.join(__dirname,'../tests/fixtures/exchange-placement-reads.json');if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'),'placement read/effect source drift');
console.log(cases.length+' source value/error/read-count cases');
