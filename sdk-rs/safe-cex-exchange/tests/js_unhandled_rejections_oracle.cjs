const unhandled = [];
process.on('unhandledRejection', error => unhandled.push(error.message));
const live = Promise.reject(new Error('unhandled-live'));
void (async () => { await null; throw new Error('unhandled-void'); })();
setTimeout(async () => { await null; throw new Error('unhandled-timer'); }, 0);
const handled = Promise.reject(new Error('handled-sync'));
handled.catch(() => {});
const deferred = Promise.reject(new Error('handled-microtasks'));
Promise.resolve().then(() => Promise.resolve().then(() => deferred.catch(() => {})));
const propagated = Promise.reject(new Error('unhandled-derived'));
void propagated.then();
const first = Promise.reject(new Error('handled-all-first'));
const later = Promise.reject(new Error('handled-all-later'));
Promise.all([first, later]).catch(() => {});
Promise.allSettled([new Promise(resolve => setTimeout(resolve, 5)), Promise.reject(new Error('handled-allSettled-later'))]);
setTimeout(() => {
  if (!live) throw Error('retain original promise');
  console.log(JSON.stringify(unhandled.sort()));
}, 20);
