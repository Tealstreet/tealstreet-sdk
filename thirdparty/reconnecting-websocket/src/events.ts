export class Event {
  public target: any;
  public type: string;
  constructor(type: string, target: any) {
    this.target = target;
    this.type = type;
  }
}

export class ErrorEvent extends Event {
  public message: string;
  public error: Error;
  constructor(error: Error, target: any) {
    super('error', target);
    this.message = error.message;
    this.error = error;
  }
}

export class CloseEvent extends Event {
  public code: number;
  public reason: string;
  public wasClean = true;
  constructor(code = 1000, reason = '', target: any) {
    super('close', target);
    this.code = code;
    this.reason = reason;
  }
}
export interface WebSocketEventMap {
  close: CloseEvent;
  error: ErrorEvent;
  message: MessageEvent;
  open: Event;
}

export interface WebSocketEventListenerMap {
  close: ((event: CloseEvent) => void | Promise<void>) | { handleEvent: (event: CloseEvent) => void | Promise<void> };
  error: ((event: ErrorEvent) => void | Promise<void>) | { handleEvent: (event: ErrorEvent) => void | Promise<void> };
  message: ((event: MessageEvent) => void | Promise<void>) | { handleEvent: (event: MessageEvent) => void | Promise<void> };
  open: ((event: Event) => void | Promise<void>) | { handleEvent: (event: Event) => void | Promise<void> };
}
