// Extract all eight unedited production methods; HTTP, socket and clock are seams.
const fs=require('node:fs'),path=require('node:path'),{ts,root}=require('./source.cjs');
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
const ast=parse('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
const names=['LISTEN_KEY_REFRESH_MS','LISTEN_KEY_FAILURE_RETRY_MS','LISTEN_KEY_WATCHDOG_INTERVAL_MS','LISTEN_KEY_WATCHDOG_STALE_MS','listenKey','lastListenKeyKeepAliveAt','listenKeyConsecutiveFailures','listenKeyRefreshTimer','listenKeyWatchdogTimer','connectRetryTimer','connectRetryCount','fetchListenKey','updateListenKey','scheduleListenKeyRefresh','clearListenKeyRefreshTimer','startListenKeyWatchdog','clearListenKeyWatchdogTimer','scheduleConnectRetry','clearConnectRetryTimer'];
const members=names.map(name=>{const node=cls.members.find(n=>n.name?.getText(ast)===name);if(!node)throw Error('Missing source member '+name);return printer.printNode(ts.EmitHint.Unspecified,node,ast);}).join('\n');
const logging=parse('packages/logging/src/LogSeverity.ts'),enums=logging.statements.filter(ts.isEnumDeclaration).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,logging)).join('\n');
const exportsObject={};new Function('exports',ts.transpileModule(enums+'\nexport class AsterDEXPrivateWebsocket {parent:any;reconnect:any;connectAndSubscribe:any;get isDisposed(){return this.parent.isDisposed;}\n'+members+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exportsObject);
const cases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/listen-key-cases.json'),'utf8'));
const realSet=setTimeout,realClear=clearTimeout,realNow=Date.now;let mono=0,wall=1000,nextId=0,timers=new Map();
global.setTimeout=(callback,delay)=>{const id=++nextId;timers.set(id,{callback,deadline:mono+delay});return id;};global.clearTimeout=id=>timers.delete(id);Date.now=()=>wall;
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{const output=[];try{for(const c of cases){mono=0;wall=1000;timers=new Map();
 const requests=[],pending={post:[],put:[],connect:[]},logs=[],errors=[],results=[];let reconnects=0;
 const rpc=(kind,requestPath,key)=>{requests.push({kind,path:requestPath,...(key===undefined?{}:{key})});return new Promise((resolve,reject)=>pending[kind].push({resolve,reject}));};
 const socket=new exportsObject.AsterDEXPrivateWebsocket();socket.parent={isDisposed:false,log:(message,severity)=>logs.push({message,severity}),getFapiPath:endpoint=>'/fapi/v3/'+endpoint,primaryAPI:{post:requestPath=>rpc('post',requestPath),put:(requestPath,data,opts)=>{if(data!==undefined)throw Error('Unexpected PUT body');return rpc('put',requestPath,opts.params.listenKey);}}};socket.reconnect=()=>{reconnects++;};socket.connectAndSubscribe=()=>{const work=rpc('connect','');work.catch(error=>errors.push(error.message));return work;};
 for(const method of ['fetchListenKey','updateListenKey']){const original=socket[method];socket[method]=(...args)=>{const work=original(...args);work.then(value=>{if(method==='fetchListenKey')results.push(value);},error=>errors.push(error.message));return work;};}
 const snapshots=[];
 for(const step of c.steps){switch(step.op){
 case 'fetch':socket.fetchListenKey();break;case 'update':socket.updateListenKey();break;
 case 'schedule-refresh':socket.scheduleListenKeyRefresh(step.delay);break;case 'clear-refresh':socket.clearListenKeyRefreshTimer();break;
 case 'start-watchdog':socket.startListenKeyWatchdog();break;case 'clear-watchdog':socket.clearListenKeyWatchdogTimer();break;
 case 'schedule-connect':socket.scheduleConnectRetry();break;case 'clear-connect':socket.clearConnectRetryTimer();break;
 case 'wall':wall=step.value;break;
 case 'tick':mono=step.value;for(const[id,timer]of [...timers])if(timers.has(id)&&timer.deadline<=mono){timers.delete(id);timer.callback();}break;
 case 'resolve':{const waiting=pending[step.kind].shift();if(!waiting)throw Error('No pending '+step.kind);if('error'in step)waiting.reject(Error(step.error));else waiting.resolve(step.kind==='post'?{data:{listenKey:step.key}}:undefined);break;}
 case 'snapshot':snapshots.push({key:socket.listenKey,last:socket.lastListenKeyKeepAliveAt,failures:socket.listenKeyConsecutiveFailures,retry:socket.connectRetryCount,refresh:socket.listenKeyRefreshTimer!==undefined,watchdog:socket.listenKeyWatchdogTimer!==undefined,connect:socket.connectRetryTimer!==undefined,timers:timers.size,requests:requests.length,reconnects});break;
 default:throw Error('Unknown step '+step.op);
 }await flush();}
 output.push({name:c.name,family:c.family,snapshots,requests,reconnects,results,logs,errors});socket.clearListenKeyRefreshTimer();socket.clearListenKeyWatchdogTimer();socket.clearConnectRetryTimer();
 }}finally{global.setTimeout=realSet;global.clearTimeout=realClear;Date.now=realNow;}process.stdout.write(JSON.stringify(output));})().catch(error=>{process.stderr.write(error.stack);process.exitCode=1;});
