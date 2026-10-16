// Executed in the original read oracle VM; uses its unchanged actual-source runner.
async function leverage(spec){
 const calls=[],effects=[],reports=[];let release;
 const pending=new Promise(resolve=>{release=resolve});
 const cache=new Map([['OLD',3.125]]);
 const consoleFailure=new Error('console failed');
 const fn=method(futuresFile,'KrakenExchange','fetchLeverageSettings',{
  ENDPOINTS:futuresTypes.ENDPOINTS,
  console:{error(...args){reports.push(capture(args));if(spec.consoleFailure)throw consoleFailure;}}
 },{
  xhr:{get(endpoint){calls.push(endpoint);if(spec.failure==='sync')throw new Error(spec.message);return pending.then(()=>{if(spec.failure==='async')throw new Error(spec.message);return {data:value(spec.raw)}});}},
  leverageSettings:{
   clear(){effects.push(['clear']);if(spec.clearFailure)throw new Error('clear failed');cache.clear();},
   set(symbol,maxLeverage){effects.push(['set',capture(symbol),capture(maxLeverage)]);if(effects.length===spec.setFailureAt)throw new Error('set failed');cache.set(symbol,maxLeverage);}
  }
 });
 const promise=fn();const prefix={calls:calls.length,effects:effects.length,reports:reports.length};
 release();let result;try{await promise;result={undefined:true}}catch(e){result={error:e.name,message:e.message,consoleIdentity:e===consoleFailure}};
 return {...spec,calls,effects,reports,prefix,result,cache:capture([...cache])};
}
globalThis.leverageSupplement = async () => {
 const leverages=[];
 for(const raw of [null,'null','false','[]','{}','{"result":"error","leveragePreferences":[]}','{"result":"success"}','{"result":"success","leveragePreferences":false}','{"result":"success","leveragePreferences":0}','{"result":"success","leveragePreferences":""}','{"result":"success","leveragePreferences":[]}','{"result":"success","leveragePreferences":"abc💰"}','{"result":"success","leveragePreferences":{}}','{"result":"success","leveragePreferences":17}',
  '{"result":"success","leveragePreferences":[{"symbol":"FIRST","maxLeverage":17.25},{"symbol":"SECOND","maxLeverage":29.5},{"symbol":"FIRST","maxLeverage":31.75}]}',
  '{"result":"success","leveragePreferences":[null]}',
  '{"result":"success","leveragePreferences":[{"symbol":"FIRST","maxLeverage":17.25},null,{"symbol":"LATER","maxLeverage":29.5}]}',
  '{"result":"success","leveragePreferences":[{},false,7,"abc",[],{"symbol":"ZERO","maxLeverage":-0},{"symbol":"NULL","maxLeverage":null},{"symbol":"EMPTY","maxLeverage":""},{"symbol":"FALSE","maxLeverage":false},{"symbol":0,"maxLeverage":17},{"symbol":false,"maxLeverage":17},{"symbol":"NEG","maxLeverage":-3.25},{"symbol":"INF","maxLeverage":1e400},{"symbol":"UNDER","maxLeverage":-1e-400},{"symbol":"TINY","maxLeverage":5e-324}]}',
  '{"result":"success","leveragePreferences":[{"symbol":"STR","maxLeverage":"17.25"},{"symbol":"BOOL","maxLeverage":true},{"symbol":"ARR","maxLeverage":[]},{"symbol":"OBJ","maxLeverage":{"v":-0}},{"symbol":7,"maxLeverage":23.5}]}'
 ,'{"result":"success","leveragePreferences":[{"symbol":{},"maxLeverage":17.25},{"symbol":{},"maxLeverage":29.5}]}'
 ,'{"result":"success","leveragePreferences":[{"symbol":[],"maxLeverage":17.25},{"symbol":[],"maxLeverage":29.5}]}'
 ])leverages.push(await leverage({raw}));
 const raw='{"result":"success","leveragePreferences":[{"symbol":"FIRST","maxLeverage":17.25},{"symbol":"SECOND","maxLeverage":29.5}]}';
 for(const failure of ['sync','async'])for(const message of ['', 'read failed'])for(const consoleFailure of [false,true])leverages.push(await leverage({raw,failure,message,consoleFailure}));
 for(const consoleFailure of [false,true]){
  leverages.push(await leverage({raw,clearFailure:true,consoleFailure}));
  leverages.push(await leverage({raw,setFailureAt:3,consoleFailure}));
 }
 const leverageBoundary=await leverage({raw:'{"result":"success","leveragePreferences":[{"symbol":"\\ud800","maxLeverage":17.25}]}'});
 leverageBoundary.symbolUnits=Array.from(leverageBoundary.effects[1][1],c=>c.charCodeAt(0));delete leverageBoundary.effects;delete leverageBoundary.cache;
 return {leverages, leverageBoundary};
};
