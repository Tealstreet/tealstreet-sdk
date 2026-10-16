// Execute installed lodash with a deterministic host clock and timer queue.
'use strict';
const throttle = require('lodash/throttle');
const cases = [];
const sequences = {
  isolated: [[0,'call',1],[40,'call',2],[80,'call',3]],
  burst: [[0,'call',1],[2,'call',2],[4,'call',3],[6,'call',4],[8,'call',5]],
  sustained: Array.from({length:30},(_,i)=>[i*2,'call',i+1]),
  cancel: [[0,'call',1],[2,'call',2],[3,'cancel'],[4,'call',3],[6,'call',4]],
  flush: [[0,'call',1],[2,'call',2],[3,'flush'],[4,'call',3],[6,'flush'],[7,'flush']],
  stalled: [[0,'call',1],[2,'call',2],[30,'stall',3],[31,'call',4]],
  rollback: [[0,'call',1],[2,'call',2],[3,'rollback',3],[4,'call',4],[20,'call',5]],
};
for (const wait of [0,5,10,'10','bad',-2]) {
  for (const options of [null,{}, {leading:false},{trailing:false}, {leading:false,trailing:false}, {leadingUndefined:true}, {trailingUndefined:true}]) {
    for (const [name,actions] of Object.entries(sequences)) {
      let now = 1000, mono = 0, id = 0;
      const timers = new Map(), trace = [];
      Date.now = ()=>now;
      global.setTimeout = (f,delay)=>{timers.set(++id,{due:mono+Math.max(0,Math.trunc(Number(delay)||0)),f});return id;};
      global.clearTimeout = id=>timers.delete(id);
      const drain = ()=>{
        for (let n=0;n<1000;n++) {
          const ready=[...timers].find(([,t])=>t.due<=mono);
          if(!ready) return;
          timers.delete(ready[0]); ready[1].f();
        }
        throw Error('timer loop');
      };
      const advance = at=>{while(mono<at){mono++;now++;drain();}};
      const opts = options === null ? undefined : {...options};
      if(opts?.leadingUndefined) opts.leading=undefined;
      if(opts?.trailingUndefined) opts.trailing=undefined;
      const f = throttle(function(...args) {
        trace.push(['invoke',now,this.id,args]);return args[0]*10;
      },wait,opts);
      for(const [at,op,value] of actions) {
        if(op==='stall') {now+=at-mono;mono=at;} else advance(at);
        if(op==='rollback') now-=50;
        let result;
        if(op==='call'||op==='stall'||op==='rollback') result=f.call({id:`receiver-${value}`},value,`arg-${value}`);
        else result=f[op]();
        trace.push([op,now,result===undefined?'undefined':result]);
      }
      advance(150);
      cases.push({wait,options,name,actions,trace});
    }
  }
}
process.stdout.write(JSON.stringify(cases));
