import { describe, it, expect, vi } from 'vitest';
import { TimeLimitedQueue } from './TimeLimitedQueue'; // Adjust the import path as necessary

describe('TimeLimitedQueue', () => {
  it('prevents items from being enqueued faster than once a second', async () => {
    const queue = new TimeLimitedQueue<string>(2); // Assuming maxSize relates to time limit calculation

    // Mock Date.now()
    const now = Date.now();
    vi.useFakeTimers();
    vi.setSystemTime(now);

    queue.enqueue('first');
    expect(queue.getSize()).toBe(1);

    // Advance time by less than a second
    vi.advanceTimersByTime(500);
    queue.enqueue('second');
    expect(queue.getSize()).toBe(1); // Size should not change, 'second' not enqueued

    // Advance time to allow enqueueing
    vi.advanceTimersByTime(500);
    queue.enqueue('second');
    expect(queue.getSize()).toBe(2); // Now 'second' should be enqueued
  });

  it('removes items older than the time limit', async () => {
    const maxSize = 2;
    const timeLimitInSeconds = maxSize * 1000; // Time limit based on maxSize
    const queue = new TimeLimitedQueue<{ value: string }>(maxSize);

    // Mock Date.now()
    const now = Date.now();
    vi.useFakeTimers();
    vi.setSystemTime(now);

    queue.enqueue({ value: 'old' }); // This item will become older than timeLimitInSeconds

    // Simulate passing of time beyond the time limit for the 'old' item
    vi.advanceTimersByTime(timeLimitInSeconds + 1000);

    // Enqueue a new item, which should trigger removal of 'old'
    queue.enqueue({ value: 'new' });
    expect(queue.getSize()).toBe(1); // Only the 'new' item should be in the queue
    expect(queue.peek()?.value).toBe('new');
  });
});
