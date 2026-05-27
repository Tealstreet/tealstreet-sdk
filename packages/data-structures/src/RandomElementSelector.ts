export class RandomElementSelector<T> {
  private shuffledArray: T[];
  private index: number = 0;
  private lastPickedValue: T | undefined;

  constructor(array: T[]) {
    this.shuffledArray = [...array];
    this.shuffleArray();
  }

  private shuffleArray(): void {
    for (let i = this.shuffledArray.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [this.shuffledArray[i], this.shuffledArray[j]] = [this.shuffledArray[j], this.shuffledArray[i]]; // Swap
    }
  }

  public getNext(): T | undefined {
    if (this.index >= this.shuffledArray.length) {
      this.reset();
    }
    const picked = this.shuffledArray[this.index++];
    if (picked === this.lastPickedValue && this.shuffledArray.length > 1) {
      return this.getNext();
    }
    this.lastPickedValue = picked;
    return picked;
  }

  public reset(): void {
    this.index = 0;
    this.shuffleArray();
  }
}
