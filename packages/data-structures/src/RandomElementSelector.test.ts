import { describe, it, expect } from 'vitest';
import { RandomElementSelector } from './RandomElementSelector'; // Adjust the import path as necessary

describe('RandomElementSelector', () => {
  it('initializes with a non-empty array and selects an element', () => {
    const array = [1, 2, 3];
    const selector = new RandomElementSelector(array);
    const element = selector.getNext();
    expect(array).toContain(element);
  });

  it('does not repeat the last picked value immediately when array length > 1', () => {
    const array = [1, 2, 3, 4, 5];
    const selector = new RandomElementSelector(array);
    let lastValue = selector.getNext();
    let newValue;
    for (let i = 0; i < 100; i++) {
      // Attempt multiple selections
      newValue = selector.getNext();
      if (newValue === lastValue) {
        // Fail test if the same value is immediately repeated
        expect(newValue).not.toBe(lastValue);
      }
      lastValue = newValue;
    }
  });

  it('cycles through all elements without repetition before reset', () => {
    const array = [1, 2, 3];
    const selector = new RandomElementSelector(array);
    const seen = new Set();

    for (let i = 0; i < array.length; i++) {
      const element = selector.getNext();
      expect(seen.has(element)).toBe(false); // Ensure no repetition
      seen.add(element);
    }

    // Next call should trigger a reset and potentially repeat
    const nextElement = selector.getNext();
    expect(array).toContain(nextElement);
    expect(seen.size).toBe(array.length); // Confirm we've seen all elements
  });

  it('resets correctly', () => {
    const array = [1, 2, 3];
    const selector = new RandomElementSelector(array);
    selector.getNext(); // Advance to the next element
    selector.getNext(); // Advance to the next element
    selector.getNext(); // Advance to the next element

    selector.reset();
    const afterReset = selector.getNext();
    expect(array).toContain(afterReset); // Should still be able to select an element
  });

  it('handles single-element arrays correctly', () => {
    const array = [1];
    const selector = new RandomElementSelector(array);
    expect(selector.getNext()).toBe(1); // Only one choice
    expect(selector.getNext()).toBe(1); // Should repeat due to reset
  });

  it('returns undefined for an empty array', () => {
    const array: number[] = [];
    const selector = new RandomElementSelector(array);
    expect(selector.getNext()).toBeUndefined();
  });
});
