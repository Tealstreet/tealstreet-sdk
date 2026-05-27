import { describe, it, expect, beforeEach } from 'vitest';
import { MemoryPreprocessStore } from './memoryStore';

describe('MemoryPreprocessStore', () => {
  let store: MemoryPreprocessStore;

  beforeEach(() => {
    store = new MemoryPreprocessStore();
  });

  describe('vars', () => {
    it('round-trips a single var', () => {
      store.setVar('size', '100$');
      expect(store.getVar('size')).toBe('100$');
    });

    it('overwrites an existing var', () => {
      store.setVar('size', '100$');
      store.setVar('size', '200$');
      expect(store.getVar('size')).toBe('200$');
    });

    it('unsetVar returns true on hit, false on miss', () => {
      store.setVar('size', '100$');
      expect(store.unsetVar('size')).toBe(true);
      expect(store.unsetVar('size')).toBe(false);
    });

    it('unsetAllVars clears everything', () => {
      store.setVar('a', '1');
      store.setVar('b', '2');
      store.unsetAllVars();
      expect(store.getVars()).toEqual({});
    });

    it('getVars returns a snapshot, not a live ref', () => {
      store.setVar('a', '1');
      const snapshot = store.getVars();
      store.setVar('b', '2');
      expect(snapshot).toEqual({ a: '1' });
    });
  });

  describe('aliases', () => {
    it('round-trips a single alias', () => {
      store.setAlias('b1', 'buy 100$');
      expect(store.getAlias('b1')).toBe('buy 100$');
    });

    it('namespaces vars and aliases independently', () => {
      store.setVar('x', '1');
      store.setAlias('x', 'buy 1');
      expect(store.getVar('x')).toBe('1');
      expect(store.getAlias('x')).toBe('buy 1');
    });

    it('unsetAlias returns true on hit, false on miss', () => {
      store.setAlias('b1', 'buy 100$');
      expect(store.unsetAlias('b1')).toBe(true);
      expect(store.unsetAlias('b1')).toBe(false);
    });

    it('unsetAllAliases clears everything', () => {
      store.setAlias('a', 'buy 1');
      store.setAlias('b', 'sell 1');
      store.unsetAllAliases();
      expect(store.getAliases()).toEqual({});
    });
  });

  describe('live vars', () => {
    it('round-trips a single live var', () => {
      store.setLiveVar('spread', '$ask-$bid');
      expect(store.getLiveVar('spread')).toBe('$ask-$bid');
    });

    it('overwrites an existing live var', () => {
      store.setLiveVar('x', '$last');
      store.setLiveVar('x', '$mark');
      expect(store.getLiveVar('x')).toBe('$mark');
    });

    it('unsetLiveVar returns true on hit, false on miss', () => {
      store.setLiveVar('x', '$last');
      expect(store.unsetLiveVar('x')).toBe(true);
      expect(store.unsetLiveVar('x')).toBe(false);
    });

    it('unsetAllLiveVars clears everything', () => {
      store.setLiveVar('a', '$last');
      store.setLiveVar('b', '$ask');
      store.unsetAllLiveVars();
      expect(store.getLiveVars()).toEqual({});
    });

    it('getLiveVars returns a snapshot, not a live ref', () => {
      store.setLiveVar('a', '$last');
      const snapshot = store.getLiveVars();
      store.setLiveVar('b', '$ask');
      expect(snapshot).toEqual({ a: '$last' });
    });

    it('live vars are namespaced separately from static vars in lookup', () => {
      // No mutex case — different names.
      store.setVar('x', '42');
      store.setLiveVar('y', '$last');
      expect(store.getVar('x')).toBe('42');
      expect(store.getLiveVar('y')).toBe('$last');
      expect(store.getVar('y')).toBeUndefined();
      expect(store.getLiveVar('x')).toBeUndefined();
    });
  });

  describe('mutual exclusion — set vs sets on the same name', () => {
    it('setLiveVar clears a same-name static binding', () => {
      store.setVar('x', '42');
      store.setLiveVar('x', '$last');
      expect(store.getVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBe('$last');
    });

    it('setVar clears a same-name live binding', () => {
      store.setLiveVar('x', '$last');
      store.setVar('x', '42');
      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getVar('x')).toBe('42');
    });

    it('flipping back and forth leaves exactly one binding', () => {
      store.setVar('x', '1');
      store.setLiveVar('x', '$ask');
      store.setVar('x', '2');
      store.setLiveVar('x', '$bid');
      expect(store.getVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBe('$bid');
      expect(Object.keys(store.getVars())).not.toContain('x');
    });

    it('unsetAllVars does NOT clear live vars', () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$ask');
      store.unsetAllVars();
      expect(store.getVar('a')).toBeUndefined();
      expect(store.getLiveVar('b')).toBe('$ask');
    });

    it('unsetAllLiveVars does NOT clear static vars', () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$ask');
      store.unsetAllLiveVars();
      expect(store.getVar('a')).toBe('1');
      expect(store.getLiveVar('b')).toBeUndefined();
    });

    it('unsetVar does not touch a same-name live var', () => {
      // After setVar('x', '1'); setLiveVar('x', '$ask') the static x is
      // gone (mutex). unsetVar('x') is a no-op for the live binding.
      store.setVar('x', '1');
      store.setLiveVar('x', '$ask');
      expect(store.unsetVar('x')).toBe(false);
      expect(store.getLiveVar('x')).toBe('$ask');
    });

    it('unsetLiveVar does not touch a same-name static var', () => {
      store.setLiveVar('x', '$ask');
      store.setVar('x', '1');
      expect(store.unsetLiveVar('x')).toBe(false);
      expect(store.getVar('x')).toBe('1');
    });
  });

  describe('tracked vars', () => {
    it('round-trips a single tracked var (initial value undefined)', () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      expect(store.getTrackedVar('hi')).toEqual({
        kind: 'max',
        source: '$pnl',
        value: undefined,
      });
    });

    it('updateTrackedVar mutates only the value field', () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      store.updateTrackedVar('hi', 42.5);
      expect(store.getTrackedVar('hi')).toEqual({
        kind: 'max',
        source: '$pnl',
        value: 42.5,
      });
    });

    it('updateTrackedVar is a no-op for non-existent names', () => {
      // Defensive: the resolver should be the only caller and only
      // when the entry exists, but a stale name shouldn't crash.
      expect(() => store.updateTrackedVar('nope', 1)).not.toThrow();
      expect(store.getTrackedVar('nope')).toBeUndefined();
    });

    it('setTrackedVar resets the value (fresh tracker on re-registration)', () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      store.updateTrackedVar('hi', 100);
      // Re-register with a new source. Value should reset to
      // undefined so the new source seeds it from scratch.
      store.setTrackedVar('hi', 'max', '$mark');
      expect(store.getTrackedVar('hi')).toEqual({
        kind: 'max',
        source: '$mark',
        value: undefined,
      });
    });

    it('unsetTrackedVar returns true on hit, false on miss', () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      expect(store.unsetTrackedVar('hi')).toBe(true);
      expect(store.unsetTrackedVar('hi')).toBe(false);
    });

    it('unsetAllTrackedVars clears every entry', () => {
      store.setTrackedVar('a', 'max', '$pnl');
      store.setTrackedVar('b', 'min', '$mark');
      store.unsetAllTrackedVars();
      expect(store.getTrackedVars()).toEqual({});
    });

    it('getTrackedVars returns a defensive copy', () => {
      store.setTrackedVar('hi', 'max', '$pnl');
      const snapshot = store.getTrackedVars();
      store.updateTrackedVar('hi', 42);
      // Snapshot was taken before the update; the snapshot value
      // shouldn't mutate after the fact.
      expect(snapshot.hi.value).toBeUndefined();
    });

    it('tracked vars are namespaced separately when names differ', () => {
      store.setVar('x', '42');
      store.setLiveVar('y', '$ask');
      store.setTrackedVar('z', 'max', '$pnl');
      expect(store.getVar('x')).toBe('42');
      expect(store.getLiveVar('y')).toBe('$ask');
      expect(store.getTrackedVar('z')).toMatchObject({ kind: 'max', source: '$pnl' });
    });
  });

  describe('three-way mutex — set vs sets vs track on the same name', () => {
    it('setTrackedVar clears a same-name static binding', () => {
      store.setVar('x', '42');
      store.setTrackedVar('x', 'max', '$pnl');
      expect(store.getVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'max' });
    });

    it('setTrackedVar clears a same-name live binding', () => {
      store.setLiveVar('x', '$ask');
      store.setTrackedVar('x', 'min', '$pnl');
      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'min' });
    });

    it('setVar clears a same-name tracked binding', () => {
      store.setTrackedVar('x', 'max', '$pnl');
      store.setVar('x', '99');
      expect(store.getTrackedVar('x')).toBeUndefined();
      expect(store.getVar('x')).toBe('99');
    });

    it('setLiveVar clears a same-name tracked binding', () => {
      store.setTrackedVar('x', 'max', '$pnl');
      store.setLiveVar('x', '$ask');
      expect(store.getTrackedVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBe('$ask');
    });

    it('flipping across all three leaves exactly one binding', () => {
      store.setVar('x', '1');
      store.setLiveVar('x', '$ask');
      store.setTrackedVar('x', 'max', '$pnl');
      expect(store.getVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'max' });
      // Cycle back to static.
      store.setVar('x', '2');
      expect(store.getTrackedVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getVar('x')).toBe('2');
    });

    it('unsetAllVars does NOT clear trackers', () => {
      store.setVar('a', '1');
      store.setTrackedVar('b', 'max', '$pnl');
      store.unsetAllVars();
      expect(store.getTrackedVar('b')).toMatchObject({ kind: 'max' });
    });

    it('unsetAllLiveVars does NOT clear trackers', () => {
      store.setLiveVar('a', '$ask');
      store.setTrackedVar('b', 'max', '$pnl');
      store.unsetAllLiveVars();
      expect(store.getTrackedVar('b')).toMatchObject({ kind: 'max' });
    });

    it('unsetAllTrackedVars does NOT clear static or live', () => {
      store.setVar('a', '1');
      store.setLiveVar('b', '$ask');
      store.setTrackedVar('c', 'max', '$pnl');
      store.unsetAllTrackedVars();
      expect(store.getVar('a')).toBe('1');
      expect(store.getLiveVar('b')).toBe('$ask');
      expect(store.getTrackedVar('c')).toBeUndefined();
    });
  });
});
