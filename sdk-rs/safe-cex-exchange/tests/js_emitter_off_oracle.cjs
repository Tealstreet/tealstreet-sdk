const Emitter = require('tiny-emitter');
const kinds = ['omitted', 'undefined', 'null', 'false', 'zero', 'empty', 'nan', 'duplicates', 'different', 'noncallable', 'missing', 'alias', 'alias-throws', 'alias-clears', 'alias-adds', 'remove-during', 'clear-during', 'native', 'clear-native', 'alias-clears-native'];
const rows = kinds.map(kind => {
  const emitter = new Emitter(), trace = [], cause = {kind};
  const second = () => trace.push('second');
  const first = () => {
    trace.push('first');
    if (kind === 'remove-during') emitter.off('fill', second);
    if (kind === 'clear-during') emitter.off('fill');
  };
  emitter.on('fill', first).on('fill', second).on('fill', second);
  emitter.on('other', () => trace.push('other'));
  if (kind.endsWith('-native') || kind === 'native') emitter.on('fill', () => trace.push('native'));
  if (kind === 'alias') {
    const wrapper = () => trace.push('wrapper');
    wrapper._ = second;
    emitter.on('fill', wrapper);
  }
  if (kind.startsWith('alias-')) Object.defineProperty(first, '_', {get() {
    if (kind === 'alias-throws') throw cause;
    if (kind.startsWith('alias-clears')) emitter.off('fill');
    if (kind === 'alias-adds') emitter.on('fill', () => trace.push('late'));
  }});
  let args = [second];
  if (kind === 'omitted') args = [];
  if (kind === 'undefined' || kind === 'clear-native') args = [undefined];
  if (kind === 'null') args = [null];
  if (kind === 'false') args = [false];
  if (kind === 'zero') args = [0];
  if (kind === 'empty') args = [''];
  if (kind === 'nan') args = [NaN];
  if (kind === 'different') args = [() => {}];
  if (kind === 'noncallable') args = [1];
  let chained = true, caught = false;
  if (!kind.endsWith('-during')) {
    try { chained = emitter.off(kind === 'missing' ? 'absent' : 'fill', ...args) === emitter; }
    catch (error) { caught = error === cause; }
  }
  emitter.emit('fill');
  if (kind.endsWith('-during')) emitter.emit('fill');
  emitter.emit('other');
  return {kind, trace, chained, caught};
});
process.stdout.write(JSON.stringify(rows));
