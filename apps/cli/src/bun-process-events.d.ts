export {};

declare global {
  namespace NodeJS {
    interface Process {
      // bun-types 1.4.x adds a direct memoryPressure overload without
      // re-declaring the generic process event overloads for `on`.
      on<Event extends keyof ProcessEventMap>(
        event: Event,
        listener: (...args: ProcessEventMap[Event]) => void,
      ): this;
      once<Event extends keyof ProcessEventMap>(
        event: Event,
        listener: (...args: ProcessEventMap[Event]) => void,
      ): this;
      addListener<Event extends keyof ProcessEventMap>(
        event: Event,
        listener: (...args: ProcessEventMap[Event]) => void,
      ): this;
      prependListener<Event extends keyof ProcessEventMap>(
        event: Event,
        listener: (...args: ProcessEventMap[Event]) => void,
      ): this;
      prependOnceListener<Event extends keyof ProcessEventMap>(
        event: Event,
        listener: (...args: ProcessEventMap[Event]) => void,
      ): this;
    }
  }
}
