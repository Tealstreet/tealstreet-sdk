class QueueNode<T> {
  value: T;
  next: QueueNode<T> | null;

  constructor(value: T) {
    this.value = value;
    this.next = null;
  }
}

export class Queue<T> {
  private head: QueueNode<T> | null = null;
  private tail: QueueNode<T> | null = null;

  enqueue(value: T): void {
    const newNode = new QueueNode(value);
    if (this.tail) {
      this.tail.next = newNode;
    }
    this.tail = newNode;
    if (!this.head) {
      this.head = newNode;
    }
  }

  dequeue(): T | null {
    if (!this.head) {
      return null;
    }

    const dequeuedValue = this.head.value;
    this.head = this.head.next;

    if (!this.head) {
      this.tail = null;
    }

    return dequeuedValue;
  }

  isEmpty(): boolean {
    return this.head === null;
  }

  peek(): T | null {
    return this.head ? this.head.value : null;
  }
}
