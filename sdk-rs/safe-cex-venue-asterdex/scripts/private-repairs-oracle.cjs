// Actual private-socket members and production DefaultStore; injected timer/RPC seams.
const fs=require('node:fs'),path=require('node:path'),{ts,root}=require('./source.cjs');
const printer=ts.createPrinter();
const print=node=>printer.printNode(ts.EmitHint.Unspecified,node,node.getSourceFile());
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
const privateAst=parse('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const cls=privateAst.statements.find(ts.isClassDeclaration);
const names=['POSITION_REPAIR_DELAY_MS','ORDER_RECONCILE_DEBOUNCE_MS','positionRepairTimer','positionRepairInFlight','positionRepairQueued','balanceReconcileTimer','balanceReconcileInFlight','balanceReconcileQueued','orderReconcileTimer','schedulePositionRepair','runPositionRepair','clearPositionRepairTimer','scheduleBalanceReconcile','runBalanceReconcile','clearBalanceReconcileTimer','scheduleOrderReconcile','runOrderReconcile','clearOrderReconcileTimer'];
const members=names.map(name=>{const member=cls.members.find(n=>n.name?.getText(privateAst)===name);if(!member)throw Error('Missing source member '+name);return print(member);}).join('\n');
const shared=parse('packages/safe-cex/src/types.ts'),logging=parse('packages/logging/src/LogSeverity.ts');const enums=[...shared.statements,...logging.statements].filter(ts.isEnumDeclaration).map(print).join('\n');
const compile=source=>ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const socketExports={};new Function('exports',compile(enums+'\nexport class AsterDEXPrivateWebsocket {parent:any; get isDisposed(){return this.parent.isDisposed;}\n'+members+'}'))(socketExports);
const cloneExports={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/clone.ts'),'utf8')))(cloneExports);
const storeAst=parse('packages/safe-cex/src/store/store.base.ts');const storeCode=storeAst.statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n');
const tsPath=process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root,'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript');
const dependencyRoot=path.dirname(path.resolve(path.dirname(require.resolve(tsPath)),'..'));
const uniqBy=require(path.join(dependencyRoot,'lodash/uniqBy'));
const storeExports={};new Function('exports','clone','virtualClock','uniqBy',compile(enums+'\n'+storeCode))(storeExports,cloneExports.clone,{getCurrentTimeMs:()=>1000},uniqBy);
const cases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/private-repairs-cases.json'),'utf8'));
const realSet=setTimeout,realClear=clearTimeout;let mono=0,nextId=0,timers=new Map();
global.setTimeout=(callback,delay)=>{const id=++nextId;timers.set(id,{callback,deadline:mono+delay});return id;};global.clearTimeout=id=>timers.delete(id);
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{const out=[];try{for(const c of cases){
 mono=0;timers=new Map();const store=new storeExports.DefaultStore();store.update(structuredClone(c.initial));
 const requests=[],pending={position:[],balance:[],order:[]},logs=[],errors=[];let notifications=0,pv=0,bv=0;
 store.subscribe(()=>{notifications++;});
 const rpc=(kind,version)=>{requests.push({kind,...(version===undefined?{}:{version})});return new Promise((resolve,reject)=>pending[kind].push({resolve,reject}));};
 const socket=new socketExports.AsterDEXPrivateWebsocket();socket.parent={isDisposed:false,store,log:(message,severity)=>logs.push({message,severity}),getPositionStateVersion:()=>pv,getBalanceStateVersion:()=>bv,fetchPositions:()=>rpc('position'),fetchOrders:()=>rpc('order'),reconcileBalanceAfterPrivateUpdate:version=>rpc('balance',version)};
 // Observe rejected detached promises without rewriting their source bodies.
 for(const name of ['runPositionRepair','runBalanceReconcile','runOrderReconcile']){const method=socket[name];socket[name]=(...args)=>{const promise=method(...args);promise.catch(error=>errors.push(error.message));return promise;};}
 const snapshots=[];
 for(const step of c.steps){switch(step.op){
  case 'schedule-position':socket.schedulePositionRepair();break;case 'run-position':socket.runPositionRepair();break;case 'clear-position':socket.clearPositionRepairTimer();break;
  case 'schedule-balance':socket.scheduleBalanceReconcile(step.version);break;case 'run-balance':socket.runBalanceReconcile(step.version);break;case 'clear-balance':socket.clearBalanceReconcileTimer();break;
  case 'schedule-order':socket.scheduleOrderReconcile();break;case 'run-order':socket.runOrderReconcile();break;case 'clear-order':socket.clearOrderReconcileTimer();break;
  case 'position-version':pv=step.value;break;case 'balance-version':bv=step.value;break;
  case 'subscriber-error':store.subscribe(()=>{throw Error(step.message);});break;
  case 'tick':mono=step.value;for(const[id,timer]of [...timers])if(timers.has(id)&&timer.deadline<=mono){timers.delete(id);timer.callback();}break;
  case 'resolve-position':case 'resolve-balance':case 'resolve-order':{const kind=step.op.slice('resolve-'.length),request=pending[kind].shift();if(!request)throw Error('Missing pending '+kind);if('error'in step)request.reject(Error(step.error));else request.resolve(kind==='balance'?step.applied:structuredClone(step.data));break;}
  case 'snapshot':snapshots.push({state:{positionTimer:socket.positionRepairTimer!==undefined,positionInFlight:socket.positionRepairInFlight,positionQueued:socket.positionRepairQueued,balanceTimer:socket.balanceReconcileTimer!==undefined,balanceInFlight:socket.balanceReconcileInFlight,balanceQueued:socket.balanceReconcileQueued,orderTimer:socket.orderReconcileTimer!==undefined},positions:structuredClone(store.positions),orders:structuredClone(store.orders),loaded:structuredClone(store.loaded),timers:timers.size,requests:requests.length,notifications});break;
  default:throw Error('Unknown step '+step.op);
 }await flush();}
 out.push({name:c.name,family:c.family,snapshots,requests,logs,errors});socket.clearPositionRepairTimer();socket.clearBalanceReconcileTimer();socket.clearOrderReconcileTimer();
 }}finally{global.setTimeout=realSet;global.clearTimeout=realClear;}process.stdout.write(JSON.stringify(out));})().catch(error=>{process.stderr.write(error.stack);process.exitCode=1;});
