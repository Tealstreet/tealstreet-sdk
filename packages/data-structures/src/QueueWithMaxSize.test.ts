import { describe, it, expect } from 'vitest';
import { QueueWithMaxSize } from './QueueWithMaxSize'; // Adjust the import path as necessary

describe('QueueWithMaxSize', () => {
  it('initializes with the correct maxSize', () => {
    const maxSize = 3;
    const queue = new QueueWithMaxSize<number>(maxSize);
    expect(queue.maxSize).toBe(maxSize);
  });

  it('enqueues items up to maxSize', () => {
    const maxSize = 2;
    const queue = new QueueWithMaxSize<number>(maxSize);
    queue.enqueue(1);
    queue.enqueue(2);
    expect(queue.isFull()).toBe(true);
  });

  it('removes the oldest item when exceeding maxSize', () => {
    const maxSize = 2;
    const queue = new QueueWithMaxSize<number>(maxSize);
    queue.enqueue(1);
    queue.enqueue(2);
    queue.enqueue(3); // This should cause 1 to be dequeued internally

    expect(queue.dequeue()).toBe(2); // The first item should now be 2
    expect(queue.dequeue()).toBe(3); // Followed by 3
    expect(queue.isEmpty()).toBe(true); // Queue should be empty now
  });

  it('correctly reports when it is full', () => {
    const maxSize = 3;
    const queue = new QueueWithMaxSize<number>(maxSize);
    queue.enqueue(1);
    queue.enqueue(2);
    expect(queue.isFull()).toBe(false); // Not yet full

    queue.enqueue(3);
    expect(queue.isFull()).toBe(true); // Now it's full
  });

  it('handles maxSize of 1 correctly', () => {
    const maxSize = 1;
    const queue = new QueueWithMaxSize<string>(maxSize);
    queue.enqueue('first');
    expect(queue.isFull()).toBe(true);
    expect(queue.peek()).toBe('first'); // Peek should show 'first'

    queue.enqueue('second'); // Should replace 'first'
    expect(queue.peek()).toBe('second'); // Now peek should show 'second'
    expect(queue.dequeue()).toBe('second'); // Dequeue should give 'second'
    expect(queue.isEmpty()).toBe(true); // Queue should be empty now
  });

  it('maintains correct size after multiple enqueue and dequeue operations', () => {
    const maxSize = 3;
    const queue = new QueueWithMaxSize<number>(maxSize);
    queue.enqueue(1);
    queue.enqueue(2);
    queue.dequeue(); // 1 out
    queue.enqueue(3);
    queue.enqueue(4); // Should cause 2 to be dequeued internally, as adding 4 exceeds maxSize

    expect(queue.dequeue()).toBe(2); // Now 2 is the first item
    expect(queue.dequeue()).toBe(3); // Followed by 3
    expect(queue.dequeue()).toBe(4); // Followed by 4
    expect(queue.isEmpty()).toBe(true); // Queue should be empty now
  });

  it('does not exceed maxSize after many operations', () => {
    const maxSize = 5;
    const queue = new QueueWithMaxSize<number>(maxSize);
    for (let i = 0; i < 100; i++) {
      queue.enqueue(i);
      expect(queue.getSize() <= maxSize).toBe(true); // Ensure size does not exceed maxSize
    }
  });
});
