type ThrottleInfo = {
  timer: NodeJS.Timeout;
  lastCallTime: number;
};

export class ThrottleManager<T extends (...args: any[]) => any> {
  private throttleMap: Map<string, ThrottleInfo> = new Map();

  constructor(
    private readonly fn: T,
    private readonly delay: number,
    private readonly getKey: (...args: Parameters<T>) => string,
  ) {}

  call(...args: Parameters<T>): void {
    const key = this.getKey(...args);
    const now = Date.now();
    const throttleInfo = this.throttleMap.get(key);

    // If no previous call or enough time has passed, execute immediately
    if (!throttleInfo || now - throttleInfo.lastCallTime >= this.delay) {
      this.fn(...args);
      this.throttleMap.set(key, {
        lastCallTime: now,
        timer: setTimeout(() => this.throttleMap.delete(key), this.delay),
      });
      return;
    }

    // Clear existing timer
    clearTimeout(throttleInfo.timer);

    // Set new timer for the tail call
    const timer = setTimeout(() => {
      this.fn(...args);
      this.throttleMap.delete(key);
    }, this.delay);

    // Update the throttle info
    this.throttleMap.set(key, {
      lastCallTime: throttleInfo.lastCallTime,
      timer,
    });
  }

  clear(): void {
    this.throttleMap.forEach((info) => clearTimeout(info.timer));
    this.throttleMap.clear();
  }
}
