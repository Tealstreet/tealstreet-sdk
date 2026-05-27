import { QueueWithMaxSize } from './QueueWithMaxSize';

export class TimeLimitedQueue<T> extends QueueWithMaxSize<T & { timestamp: number }> {
  private readonly timeLimitInSeconds: number;
  private lastEnqueueTime: number = 0;

  constructor(maxSize: number) {
    super(maxSize);
    this.timeLimitInSeconds = maxSize * 1_000;
  }

  enqueue(value: T): void {
    const currentTime = Date.now(); // Get current time in seconds

    // Prevent enqueueing faster than once a second
    if (currentTime - this.lastEnqueueTime < 1000) {
      return;
    }

    // Remove old elements
    while (!this.isEmpty() && currentTime - this.peek()!.timestamp >= this.timeLimitInSeconds) {
      super.dequeue();
    }

    super.enqueue({ ...value, timestamp: currentTime });
    this.lastEnqueueTime = currentTime; // Update last enqueue time
  }
}
