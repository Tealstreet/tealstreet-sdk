import { Queue } from './Queue';

export class QueueWithMaxSize<T> extends Queue<T> {
  public readonly maxSize: number;
  protected size: number = 0;

  constructor(maxSize: number) {
    super();
    this.maxSize = maxSize;
  }

  enqueue(value: T): void {
    if (this.isFull()) {
      this.dequeue(); // If the queue is full, dequeue an item before enqueueing a new one.
    } else {
      this.size++; // Increment size only if the queue was not full.
    }
    super.enqueue(value); // Enqueue the new item.
  }

  dequeue(): T | null {
    if (!this.isEmpty()) {
      this.size--; // Decrement size when an item is dequeued.
    }
    return super.dequeue();
  }

  isFull(): boolean {
    return this.size === this.maxSize;
  }

  isEmpty(): boolean {
    return this.size === 0; // Overriding isEmpty to use our size tracking.
  }

  getSize(): number {
    return this.size;
  }
}
