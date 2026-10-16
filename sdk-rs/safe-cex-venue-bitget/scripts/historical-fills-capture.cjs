// Actual history method, fill mapper and shared splitter, executed once per corpus.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
function ast(file){return ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);}
function evaluate(body,bindings={}){const exports={};new Function('exports',...Object.keys(bindings),ts.transpileModule(body,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));return exports;}
function load(file,bindings={},enums=false){return evaluate(ast(file).statements.filter(n=>enums?ts.isEnumDeclaration(n):!ts.isImportDeclaration(n)).map(n=>n.getText()).join('\n'),bindings);}
const enums=load('packages/safe-cex/src/types.ts',{},true);
const fills=load('packages/safe-cex/src/exchanges/bitget/bitget.fills.ts',enums);
const splitter=load('packages/safe-cex/src/utils/splitTimeRange.ts');
const source=ast('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const member=klass.members.find(n=>n.name?.getText(source)==='fetchHistoricalFills');
if(!member)throw Error('missing fetchHistoricalFills');
const constants=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>/^BITGET_FILLS_/.test(d.name.getText(source))));
const {Source}=evaluate(constants.map(n=>n.getText(source)).join('\n')+'\nexport class Source{'+member.getText(source)+'}',{...fills,...splitter});
function encode(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(v)?NaN:v);return {$number:Object.is(v,-0)?'-0':String(v),bits:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(v))return v.map(encode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)]));return v;}
const fill=(id='a',extra={})=>({symbol:'BTC',cTime:'1700000000123',baseVolume:'2.5',price:'11.25',side:'sell',tradeSide:'close',profit:'3.75',fee:'-0.125',tradeId:id,...extra});
const page=(rows=[fill()],endId)=>({rows,endId});
const span=89*24*60*60*1000;
(async()=>{
 const cases=[];
 async function capture(name,config={}){
  const capturedConfig=encode(config);
  const calls=[],instance=new Source(),opts=config.opts||{},pages=config.pages||[page()];let index=0,products=0;
  instance.isPublicDataOnly=!!config.public;
  instance.ensureAccountTypeDetected=async()=>{calls.push(['detect']);if(config.detectError)throw Error('detect failure');};
  Object.defineProperty(instance,'usesUtaRouting',{get(){calls.push(['uta']);return !!config.uta;}});
  instance._resolveMarket=symbol=>{calls.push(['resolve',symbol]);if(symbol==='THROW')throw Error('resolve failure');return symbol==='MISSING'?undefined:{id:symbol==='EMPTY'?'':symbol==='BTC'?'BTCUSDT':symbol,symbol:symbol==='EMPTY'?'':symbol==='BTC'?'BTC/USDT':symbol};};
  Object.defineProperty(instance,'apiProductType',{get(){calls.push(['product',products++]);if(config.productError)throw Error('product failure');return products%2?'USDT-FUTURES':'USDC-FUTURES';}});
  instance._fetchFillsPage=async params=>{calls.push(['read',params]);const current=config.cycle?pages[index++%pages.length]:pages[index++];if(current?.error)throw current.error;return current||page([]);};
  instance.emitAndLogError=message=>{calls.push(['emit',message]);if(config.reportError)throw Error('report failure');};
  let output;try{output={value:await instance.fetchHistoricalFills(opts)};}catch(e){output={error:{name:e.name,message:e.message}};}
  cases.push({name,config:capturedConfig,calls:encode(calls),output:encode(output)});
 }
 await capture('baseline');
 await capture('derived-fee-infinity',{pages:[page([fill('overflow',{feeDetail:[{totalFee:'1e308'},{totalFee:'1e308'}]})])]});
 await capture('native-negative-zero-output',{pages:[page([fill('negative-zero',{fee:'0',profit:'-0'})])]});
 await capture('native-positive-zero-output',{pages:[page([fill('positive-zero',{fee:'-0',profit:'0'})])]});
 for(const publicOnly of [false,true])for(const uta of [false,true])for(const detectError of [false,true])await capture(`guards:${publicOnly}:${uta}:${detectError}`,{public:publicOnly,uta,detectError});
 for(const symbol of ['', 'BTC','EMPTY','MISSING','THROW','Σİſ😀\0','\ufeff','\u0085'])await capture('symbol:'+symbol,{opts:{symbol}});
 for(const [from,to] of [[undefined,undefined],[0,undefined],[undefined,0],[-0,0],[0,-0],[-5,2],[2,1],[0,span],[0,span+1],[0,span*2+2],[Infinity,2],[0,Infinity],[NaN,1]])await capture('window:'+String(from)+':'+String(to),{opts:{from,to},pages:[page(),page(),page()]});
 for(const limit of [undefined,0,-0,1,99,100,101,-1,NaN,Infinity,-Infinity])await capture('limit:'+String(limit),{opts:{limit}});
 await capture('short-page-continues',{pages:[page([fill('a')],'next'),page([fill('b')])]});
 await capture('empty-stops',{pages:[page([],'next'),page([fill('b')])]});
 await capture('cursor-repeat',{pages:[page([fill('a')],'same'),page([fill('b')],'same'),page([fill('c')])]});
 await capture('cursor-cycle-cap',{cycle:true,pages:[page([fill('a')],'x'),page([fill('b')],'y')]});
 await capture('global-window-dedup',{opts:{from:0,to:span+1},pages:[page([fill('a')]),page([fill('a',{price:'99'}),fill('b')])]});
 await capture('empty-id-is-present',{pages:[page([fill([]),fill([],{price:'99'}),fill(undefined,{tradeId:undefined})])]});
 await capture('fallback-first-wins',{pages:[page([fill(undefined,{tradeId:undefined}),fill(undefined,{tradeId:undefined,profit:'900'}),fill('named')])]});
 await capture('full-page-map-before-append',{pages:[page([fill('prior')],'next'),page([fill('lost'),fill('bad',{price:{toString:1}})])]});
 await capture('resolve-failure-in-map',{pages:[page([fill('prior')],'next'),page([fill('lost'),fill('bad',{symbol:'THROW'})])]});
 await capture('product-error',{productError:true});
 const values=[undefined,null,false,true,0,'','detail',[],{}, {toString:1}];
 for(let i=0;i<values.length;i++)for(const reportError of [false,true])await capture(`catch:${i}:${reportError}`,{reportError,pages:[page([fill('prior')],'next'),{error:{response:{data:{msg:values[i]}},message:'fallback'}}]});
 await capture('undefined-error-message',{pages:[{error:{}}]});
 for(let i=0;i<values.length;i++)await capture(`raw-catch-message:${i}`,{pages:[{error:{message:values[i]}}]});
 await capture('duplicate-still-maps-before-dedup',{pages:[page([fill('a')],'next'),page([fill('a',{price:{toString:1}})])]});
 await capture('filtered-row-still-advances',{pages:[page([fill('bad',{symbol:'MISSING'})],'next'),page([fill('good')])]});
 const aliases=[];
 const aliased=new Source(),mutableOpts={symbol:'BTC',limit:1};let seen;
 aliased.ensureAccountTypeDetected=async()=>{mutableOpts.symbol='ETH';mutableOpts.limit=7;};
 aliased._resolveMarket=symbol=>({id:symbol,symbol});aliased.apiProductType='USDT-FUTURES';
 aliased._fetchFillsPage=async params=>{seen=params;return page([]);};
 await aliased.fetchHistoricalFills(mutableOpts);aliases.push(encode(seen));
 const utf16=[];
 for(const symbol of ['\ud800','x\udc00']){
  const instance=new Source();instance.ensureAccountTypeDetected=async()=>{};instance.apiProductType='USDT-FUTURES';
  instance._resolveMarket=symbol=>({id:symbol,symbol});instance._fetchFillsPage=async()=>page([fill('x',{symbol})]);
  const rows=await instance.fetchHistoricalFills({symbol});utf16.push({inputJson:JSON.stringify(symbol),outputJson:JSON.stringify(rows[0].symbol)});
 }

 process.stdout.write(JSON.stringify({cases,aliases,utf16})+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
