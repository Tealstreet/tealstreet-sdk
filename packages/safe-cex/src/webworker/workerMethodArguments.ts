import type { CallableMethodsInWorker, WORKER_OPTIONAL_METHODS } from '../exchanges/base';
import type { RustEngineExchange } from '../exchanges/rust/rust.exchange';

export type WorkerMethodName = CallableMethodsInWorker | (typeof WORKER_OPTIONAL_METHODS)[number];
export type WorkerMethodArguments = {
  [Method in WorkerMethodName]: Parameters<NonNullable<RustEngineExchange[Method]>>;
};
