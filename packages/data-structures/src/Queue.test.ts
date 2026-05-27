import { describe, it, expect } from 'vitest';
import { Queue } from './Queue';

describe('Queue', () => {
  it('starts empty', () => {
    const queue = new Queue<number>();
    expect(queue.isEmpty()).toBe(true);
  });

  it('enqueues items', () => {
    const queue = new Queue<string>();
    queue.enqueue('first');
    expect(queue.isEmpty()).toBe(false);
    expect(queue.peek()).toBe('first');

    queue.enqueue('second');
    expect(queue.peek()).toBe('first'); // Still 'first' at the front
  });

  it('dequeues items in the correct order', () => {
    const queue = new Queue<number>();
    queue.enqueue(1);
    queue.enqueue(2);
    queue.enqueue(3);

    expect(queue.dequeue()).toBe(1);
    expect(queue.dequeue()).toBe(2);
    expect(queue.dequeue()).toBe(3);
    expect(queue.isEmpty()).toBe(true);
  });

  it('handles dequeue from an empty queue correctly', () => {
    const queue = new Queue<number>();
    expect(queue.dequeue()).toBeNull();
  });

  it('correctly implements peek functionality', () => {
    const queue = new Queue<string>();
    expect(queue.peek()).toBeNull(); // Empty queue

    queue.enqueue('first');
    expect(queue.peek()).toBe('first');

    queue.enqueue('second');
    expect(queue.peek()).toBe('first'); // 'first' is still at the front

    queue.dequeue();
    expect(queue.peek()).toBe('second'); // Now 'second' is at the front
  });

  it('becomes empty after dequeuing the only item', () => {
    const queue = new Queue<boolean>();
    queue.enqueue(true);
    expect(queue.isEmpty()).toBe(false);

    queue.dequeue();
    expect(queue.isEmpty()).toBe(true);
  });

  it('maintains correct state with multiple enqueue and dequeue operations', () => {
    const queue = new Queue<number>();
    queue.enqueue(1);
    queue.enqueue(2);
    queue.dequeue(); // 1 out
    queue.enqueue(3);

    expect(queue.dequeue()).toBe(2);
    expect(queue.dequeue()).toBe(3);
    expect(queue.isEmpty()).toBe(true);
  });
});
