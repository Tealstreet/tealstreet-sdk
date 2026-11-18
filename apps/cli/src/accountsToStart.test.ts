import { describe, expect, it } from 'vitest';

import { accountsToStart } from './accountsToStart';

const none = () => false;

describe('accountsToStart', () => {
  it('connects an inactive account named with -a', () => {
    const b5 = { name: 'b5', active: false };
    expect(accountsToStart(b5, true, [], none)).toEqual([b5]);
  });

  it('leaves an inactive saved primary alone when nothing named it', () => {
    expect(accountsToStart({ name: 'b5', active: false }, false, [], none)).toEqual([]);
  });

  it('puts the primary first, once, and skips what is connected', () => {
    const main = { name: 'main', active: true };
    const other = { name: 'other', active: true };
    const live = { name: 'live', active: true };
    expect(accountsToStart(main, false, [other, main, live], (name) => name === 'live')).toEqual([main, other]);
  });
});
