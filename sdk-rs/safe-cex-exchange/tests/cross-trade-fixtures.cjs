const copier=(extra={})=>({enabled:true,account:'c',weight:1,execution:'Market',weightMode:'Absolute',symbolMappings:{},...extra});
const config=(extra={})=>({enabled:true,masterAccount:'m',masterWeight:1,copiers:[],...extra});
const market=(symbol='BTCUSDT',base='BTC',extra={})=>({id:symbol,symbol,base,quote:'USDT',active:true,precision:{amount:0.01,price:0.5},limits:{amount:{min:0.01,max:1000},leverage:{min:1,max:100}},...extra});
const ticker=(symbol='BTCUSDT',last=100)=>({id:symbol,symbol,last,bid:last,ask:last,mark:last,index:last,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0});
const position=(contracts=1,side='long',symbol='BTCUSDT',extra={})=>({id:symbol,symbol,side,entryPrice:100,notional:100,leverage:1,unrealizedPnl:0,contracts,liquidationPrice:0,...extra});
const chaser=(size=1,extra={})=>({id:'chaser',symbol:'BTCUSDT',side:'buy',size,maxAttempts:10,attempts:0,isStopped:false,stopReason:'',...extra});
const store=(positions=[],extra={})=>({positionsLoaded:true,positions,markets:[market()],tickers:[ticker()],tickersBySymbol:{BTCUSDT:ticker()},balance:{total:100,free:80,used:20,upnl:0},activeChasers:[],...extra});
const mappings=[
 {label:'forward mapping',mappings:{BTCUSDT:'CUSTOM'},fixed:undefined,symbol:'BTCUSDT',forward:true},
 {label:'empty mapping fallback',mappings:{BTCUSDT:''},symbol:'BTCUSDT',forward:true},
 {label:'forward conversion',mappings:{},symbol:'BTCUSDT',forward:true},
 {label:'no destination',mappings:{},symbol:'BTCUSDT',forward:true,noExchange:true},
 {label:'fixed master wins',mappings:{BTCUSDT:'CUSTOM'},fixed:'FIXED',symbol:'CUSTOM'},
 {label:'reverse map',mappings:{BTCUSDT:'CUSTOM'},symbol:'CUSTOM'},
 {label:'last duplicate wins',mappings:{FIRST:'CUSTOM',LAST:'CUSTOM'},symbol:'CUSTOM'},
 {label:'numeric own key order',mappings:{'2':'CUSTOM','1':'CUSTOM',JA:'CUSTOM'},symbol:'CUSTOM'},
 {label:'reverse empty key fallback',mappings:{'':'BTCUSDT'},symbol:'BTCUSDT'},
 {label:'reverse conversion',mappings:{},symbol:'BTCUSDT'},
 {label:'reverse no exchange',mappings:{},symbol:'BTCUSDT',noExchange:true},
 {label:'empty fixed fallback',mappings:{BTCUSDT:'CUSTOM'},fixed:'',symbol:'CUSTOM'},
];
const placeCases=[
 ['market','Market','1','100',[]],['market zero','Market','0','100',[]],['market negative','Market','-2','100',[]],['market NaN','Market','NaN','100',[]],['market Infinity','Market','Infinity','100',[]],
 ['limit buy','LimitSlippage','2','100',[]],['limit sell','LimitSlippage','2','100',[],{side:'sell'}],['limit missing ref','LimitSlippage','1','undefined',[]],['limit NaN ref','LimitSlippage','1','NaN',[]],['limit zero amount','LimitSlippage','0','100',[]],['limit default precision','LimitSlippage','1','100',[],{precision:0}],
 ['chase fresh','Chase','1','100',[]],['chase same size','Chase','1','100',[chaser()]],['chase changed size','Chase','2','100',[chaser()]],['chase zero terminates','Chase','0','100',[chaser()]],['chase multiple','Chase','2','100',[chaser(),chaser(3,{id:'second'})]],['chase unrelated','Chase','1','100',[chaser(2,{symbol:'ETHUSDT'}),chaser(2,{isStopped:true}),chaser(2,{side:'sell'})]],
 ['market error','Market','1','100',[],{fail:'order'}],['limit error','LimitSlippage','1','100',[],{fail:'order'}],['warning replaces error','Market','1','100',[],{fail:'warning'}],['absent exchange','Market','1','100',[],{absent:true}],['negative zero','Market','-0','100',[]]
].map(([label,execution,amount,reference,activeChasers,extra={}])=>({label,copier:copier({execution}),market:market('BTCUSDT','BTC',{precision:{amount:0.01,price:extra.precision??0.5}}),amount,reference,activeChasers,side:extra.side??'buy',reduceOnly:true,...extra}));
const audits=[];
function audit(label,kind,masterPositions,copyPositions,extra={},slots=1){audits.push({label,kind,config:config(),copier:copier(),master:store(masterPositions),copy:store(copyPositions),slots,...extra});}
audit('orphan long close','copier',[],[position()]);
audit('orphan short native close','copier',[],[position(1,'short','BTCUSDT',{extra:{contractsNative:3}})]);
audit('increase','copier',[position(2)],[position()]);
audit('decrease below minimum close','copier',[position(0.005)],[position(0.02)]);
audit('exact five percent','copier',[position(21)],[position(20)]);
audit('below five percent','copier',[position(20.5)],[position(20)]);
audit('hedged short add','copier',[position(2)],[position(1,'short')],{copier:copier({weight:-1})});
audit('explicit selected mapping closes other','copier',[position()],[position()],{copier:copier({symbolMappings:{BTCUSDT:'OTHER'}})});
audit('master filter ignores other','copier',[],[position(1,'long','ETHUSDT')],{config:config({masterSymbol:'BTCUSDT'})},0);
audit('cross base conversion','copier',[position(2)],[position(1,'long','ETHUSDT')],{copier:copier({symbolMappings:{BTCUSDT:'ETHUSDT'}}),copy:store([position(1,'long','ETHUSDT')],{markets:[market('ETHUSDT','ETH')],tickers:[ticker('ETHUSDT',200)]})});
audit('missing copier market','copier',[],[position()],{copy:store([position()],{markets:[]})});
audit('entry reference, missing order price','copier',[],[position()],{master:store([],{tickers:[]}),copy:store([position()],{tickers:[]})});
audit('open long','master',[position(2)],[]);
audit('open short hedge','master',[position(2,'short')],[],{copier:copier({weight:-1})});
audit('already open','master',[position()],[position()]);
audit('cross base open','master',[position(2)],[],{copier:copier({symbolMappings:{BTCUSDT:'ETHUSDT'}}),copy:store([],{markets:[market('ETHUSDT','ETH')],tickers:[ticker('ETHUSDT',200)]})});
audit('zero weight no open','master',[position()],[],{copier:copier({weight:0})});
audit('zero contracts omitted','master',[position(0)],[],{},0);
audit('absent account','master',[position()],[],{copier:copier({account:''})},0);
audit('two requests eager','master',[position(),position(2,'short')],[],{},2);
audit('native inverse contracts','master',[position(2)],[],{copy:store([],{markets:[market('BTCUSDT','BTC',{isInverse:true,contractSize:10})]})});
audit('entry reference open','master',[position()],[],{master:store([position()],{tickers:[]}),copy:store([],{tickers:[]})});
const stockStore=(positions,symbol)=>store(positions,{markets:[market(symbol)],tickers:[ticker(symbol)]});
audit('stock constructor skips master order','master',[position(1,'long','constructor')],[],{master:stockStore([position(1,'long','constructor')],'constructor'),copy:stockStore([],'constructor'),expectedCalls:0});
audit('stock constructor filters copier','copier',[],[position(1,'long','constructor')],{config:config({masterSymbol:'constructor'}),master:stockStore([],'constructor'),copy:stockStore([position(1,'long','constructor')],'constructor'),expectedCalls:0},0);
audit('invert proto setter skips close','copier',[],[position(1,'long','__proto__')],{copier:copier({symbolMappings:{BTCUSDT:'__proto__'}}),copy:stockStore([position(1,'long','__proto__')],'__proto__'),expectedCalls:0});
audit('own constructor shadow opens','master',[position(1,'long','constructor')],[],{copier:copier({symbolMappings:{constructor:'COPIER'}}),master:stockStore([position(1,'long','constructor')],'constructor'),copy:stockStore([],'COPIER'),expectedCalls:1});
const weights=[];
for(const weight of ['-2','0','3','NaN','Infinity','-0'])for(const masterWeight of ['1','0'])weights.push({weight,masterWeight,mode:'Absolute',master:store(),copy:store(),masterMarket:market(),copierMarket:market()});
weights.push(
 {weight:'2',masterWeight:'1',mode:'Balance',master:store(),copy:store([],{balance:{total:50,used:0,free:0,upnl:0}}),masterMarket:market(),copierMarket:market()},
 {weight:'1',masterWeight:'1',mode:'Balance',master:store(),copy:store([],{balance:{total:0,used:0,free:0,upnl:0}}),masterMarket:market(),copierMarket:market()},
 {weight:'1',masterWeight:'1',mode:'Balance',master:store(),copy:store(),masterMarket:market(),copierMarket:market('ETHUSDC','ETH',{quote:'USDC'})},
 {weight:'1',masterWeight:'1',mode:'Balance',master:store(),copy:store(),masterMarket:market('BTCUSD','BTC',{quote:'USD',isInverse:true}),copierMarket:market()},
 {weight:'1',masterWeight:'1',mode:'Balance',master:store([],{tickersBySymbol:{BTCUSD:ticker('BTCUSD',100)}}),copy:store(),masterMarket:market('BTCUSD','BTC',{quote:'USD',isInverse:true}),copierMarket:market()},
 {weight:'1',masterWeight:'1',mode:'Balance',master:store(),copy:store([],{tickers:[ticker('ETHUSDT',20),ticker('XBTUSD',100),ticker('BTCUSD',999)]}),masterMarket:market(),copierMarket:market('ETHUSDT','ETH',{quote:'USD',info:{isQuanto:true}})},
 {weight:'1',masterWeight:'NaN',mode:'Absolute',master:store(),copy:store(),masterMarket:market(),copierMarket:market()}
);
module.exports={copier,config,market,ticker,position,store,mappings,placeCases,audits,weights};
