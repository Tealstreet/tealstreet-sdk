const {BreakoutpropExchange,store,encode,observe}=require('./portfolio_oracle.cjs');
const assert=require('node:assert/strict');
const instrument=(symbol,n)=>({symbol,type:'FOREX',version:n+.125,description:'desc-'+n,priceIncrement:n+.25,pipSize:n+.375,currency:'USD',lotSize:n+.5,multiplier:n+.625,firstCurrency:'base-'+n,opaque:{zero:-0,overflow:Infinity}});
const detail=(symbol,n)=>({symbol,account:'acc-'+n,type:'FOREX',tradingStatus:'FULL',minOrderSize:n+.125,maxOrderSize:{value:n+.25,limitType:'ABS'},minOrderSizeIncrement:n+.375,marginRate:{rateType:'RATE',tiers:[{endVolume:'INF',value:n+.5}]},opaque:{negative:-Infinity}});
const specs=[
 ['ordered-duplicates',()=>{}],
 ['empty-instruments',(l)=>{l.instruments=[];}],
 ['empty-details',(l,r)=>{r.instrumentDetails=[];}],
 ['no-match',(l,r)=>{r.instrumentDetails.forEach(d=>d.symbol='MISS');}],
 ...[null,false,0,-0,''].flatMap((v,i)=>[['left-falsy-'+i,()=>{},v,undefined],['right-falsy-'+i,()=>{},undefined,v]]),
 ['unicode',(l,r)=>{l.instruments[0].symbol='é😀USD';r.instrumentDetails[1].symbol='é😀USD';}],
 ['left-reject',()=>{}],['right-reject',()=>{}],
 ['missing-unused',(l)=>{delete l.instruments[0].description;}],
 ['surrogate',(l,r)=>{l.instruments[0].symbol='\ud800';r.instrumentDetails[1].symbol='\ud800';}],
];
(async()=>{
 const cases=[];
 for(const [name,mutate,lv,rv] of specs){
  let l={instruments:[instrument('A',11),instrument('B',23),instrument('A',37)]};
  let r={instrumentDetails:[detail('B',41),detail('A',53),detail('MISS',67),detail('A',79)]};mutate(l,r);
  if(lv!==undefined)l=lv;if(rv!==undefined)r=rv;
  const ex=new BreakoutpropExchange('breakoutprop',{},store()),trace=[];
  const error=Object.assign(new Error('original-rejection'),{store_subscriber_ids:[19,31]});
  ex.fetchInstrumentsRaw=()=>{trace.push('instruments');return name==='left-reject'?Promise.reject(error):Promise.resolve(l);};
  ex.fetchInstrumentDetailsRaw=()=>{trace.push('details');return name==='right-reject'?Promise.reject(error):Promise.resolve(r);};
  let expected;
  try{expected={ok:observe(await ex.fetchCombinedInstruments())};}catch(e){expected={error:e.message,sameError:e===error};}
  if(name==='surrogate')expected.ok=expected.ok.map(row=>({instrumentUnit:row.instrument.symbol.charCodeAt(0),detailUnit:row.instrumentDetails.symbol.charCodeAt(0)}));
  cases.push({name,left:encode(l),right:encode(r),trace,expected});
 }
 // Source Promise keeps the other request alive after rejection and discarded results.
 const lifecycle=[];
 for(const mode of ['discard','reject-left','reject-right']){
  const ex=new BreakoutpropExchange('breakoutprop',{},store());let settleLeft,settleRight,finished=0;
  ex.fetchInstrumentsRaw=()=>new Promise((resolve,reject)=>{settleLeft=()=>{finished++;mode==='reject-left'?reject(Error('left')):resolve({instruments:[]});};});
  ex.fetchInstrumentDetailsRaw=()=>new Promise((resolve,reject)=>{settleRight=()=>{finished++;mode==='reject-right'?reject(Error('right')):resolve({instrumentDetails:[]});};});
  let caught=false;
  const promise=ex.fetchCombinedInstruments().catch(()=>{caught=true;});
  if(mode==='reject-left'){settleLeft();await promise;assert(caught);settleRight();}
  else if(mode==='reject-right'){settleRight();await promise;assert(caught);settleLeft();}
  else{settleRight();settleLeft();await promise;}
  lifecycle.push({name:mode,finished,caught});
 }
 const scheduleEx=new BreakoutpropExchange('breakoutprop',{},store());let rejectL,rejectR;
 scheduleEx.fetchInstrumentsRaw=()=>new Promise((_,reject)=>{rejectL=reject;});
 scheduleEx.fetchInstrumentDetailsRaw=()=>new Promise((_,reject)=>{rejectR=reject;});
 const scheduled=scheduleEx.fetchCombinedInstruments().catch(e=>e.message);await new Promise(setImmediate);rejectR(Error('right'));rejectL(Error('left'));
 const rejectionOrder=await scheduled;
 const ex=new BreakoutpropExchange('breakoutprop',{},store());const l={instruments:[instrument('A',1)]},r={instrumentDetails:[detail('A',2)]};
 ex.fetchInstrumentsRaw=async()=>l;ex.fetchInstrumentDetailsRaw=async()=>r;
 const result=await ex.fetchCombinedInstruments();result[0].instrument.description='changed';result[0].instrumentDetails.account='changed';
 const aliases={instrument:l.instruments[0].description,details:r.instrumentDetails[0].account};
 assert.equal(cases.length,19);
 process.stdout.write(JSON.stringify({runtime:{node:process.version,nodeMajor:+process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},cases,lifecycle,aliases,rejectionOrder}));
})().catch(e=>{console.error(e);process.exitCode=1;});
