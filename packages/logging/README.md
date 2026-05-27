# @tealstreet/logging

This package provides logging utilities for the Tealstreet platform, including integration with Grafana and Sentry.

## Debug Logger

The `createDebugLogger` utility provides a standardized way to add debug logging to any component or module. It handles:

- Conditional console logging based on dev overrides or explicit flags
- Automatic Grafana logging with proper severity levels
- Safe message formatting with circular reference protection
- Sensitive data redaction (keys, secrets, passwords, tokens)
- Sentry integration for error tracking

### Usage

```typescript
import { createDebugLogger, DebugLevel } from '@tealstreet/logging/src/debugLogger';
import { getDevOverrides } from '@/utils/developerOverrides';

// Create a debug logger with dev override
const debug = createDebugLogger({
  prefix: 'MyComponent',
  shouldLogToConsole: () => getDevOverrides().myComponentLogging
});

// Always log to console
const debug = createDebugLogger({
  prefix: 'MyComponent',
  enableConsoleLog: true
});

// Use the logger
debug(DebugLevel.Info, 'Component initialized');
debug(DebugLevel.Warning, 'Something unexpected happened', { data: 123 });
debug(DebugLevel.Error, 'An error occurred', error);
debug(DebugLevel.Local, 'This only logs to console, not Grafana');
```

### Debug Levels

- `DebugLevel.Local` - Only logs to console, not sent to Grafana
- `DebugLevel.Info` - Informational messages
- `DebugLevel.Warning` - Warning messages
- `DebugLevel.Error` - Error messages (also sent to Sentry if an Error object is provided)

### Features

1. **Safe Object Serialization**: Objects are automatically serialized with circular reference protection
2. **Sensitive Data Redaction**: Fields containing 'key', 'secret', 'password', or 'token' are automatically redacted
3. **Length Limiting**: Long messages are truncated to prevent excessive log sizes
4. **Error Tracking**: Error objects are automatically sent to Sentry with full stack traces while Grafana receives only the error message
5. **Development-Friendly**: External logging (Grafana/Sentry) is disabled in development by default

### Environment Configuration

By default, the debug logger only sends logs to external services (Grafana and Sentry) in production. In development, logs only appear in the console.

To control external logging:
- In production: Grafana logging is enabled by default
- In development: Set `NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING=1` to enable Grafana logging
- Set `NEXT_PUBLIC_FORCE_SENTRY=1` to enable Sentry error tracking in development

### Web Worker Usage

The debug logger works in Web Workers, but requires additional configuration since `process.env` is not available:

```typescript
// In Web Worker initialization
const debug = createDebugLogger({
  prefix: 'MyWorker',
  isProduction: settings.isProduction, // Pass from main thread
  forceExternalLogging: settings.forceExternalLogging,
  shouldLogToConsole: () => settings.enableWorkerLogging
});
```

In the main thread, pass the environment configuration when initializing the worker:

```typescript
worker.postMessage({
  op: 'init',
  data: {
    settings: {
      isProduction: process.env.NODE_ENV === 'production',
      forceExternalLogging: process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING === '1',
      enableWorkerLogging: getDevOverrides().workerLogging
    }
  }
});
```

### Migration Guide

To migrate existing debug functions to use the new logger:

1. Replace custom debug function with `createDebugLogger`:

```typescript
// Before
const debug = (...args: any[]) => 
  getDevOverrides().myLogging && console.log('[MyComponent]', ...args);

// After
const debug = createDebugLogger({
  prefix: 'MyComponent',
  shouldLogToConsole: () => getDevOverrides().myLogging
});
```

2. Update all debug calls to include a severity level:

```typescript
// Before
debug('Something happened');

// After
debug(DebugLevel.Info, 'Something happened');
```

3. For errors, use `DebugLevel.Error`:

```typescript
// Before
debug('Error:', error);

// After
debug(DebugLevel.Error, 'Error occurred', error);
```

## When to Use Each Logging Method

### Use Exchange Logging Methods for Exchange Classes

For any code that extends `BaseExchange`, use the built-in logging methods:

#### Basic Logging: `this.log()`
```typescript
class BinanceExchange extends BaseExchange {
  async fetchBalance() {
    this.log('Fetching balance', LogSeverity.Info);
    
    try {
      const balance = await this.api.getBalance();
      this.log(`Balance fetched: ${balance.total} USDT`, LogSeverity.Info);
      return balance;
    } catch (error) {
      this.log(`Failed to fetch balance: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  }
}
```

#### Error Logging with Sentry: `this.logError()`
```typescript
class BinanceExchange extends BaseExchange {
  async placeOrder(opts: PlaceOrderOpts) {
    try {
      const result = await this.api.createOrder(opts);
      this.log(`Order placed: ${result.id}`, LogSeverity.Info);
      return result;
    } catch (error) {
      // This will log to Grafana AND send to Sentry (in production)
      this.logError(`Order placement failed for ${opts.symbol}`, error);
      throw error;
    }
  }
}
```

#### Critical Errors: `this.emitAndLogError()`
```typescript
class BinanceExchange extends BaseExchange {
  handleWebSocketError(error: Error) {
    // This emits an error event, logs to console, and sends to Sentry
    this.emitAndLogError(error, 'WebSocket connection failed');
  }
}
```

**Why use exchange methods?** The exchange logging system:
- Already integrates with Grafana through event emitters
- Works seamlessly in Web Workers
- Provides consistent prefixing with exchange/account names
- Now respects the same environment rules as `createDebugLogger`
- Automatically sends errors to Sentry (when appropriate)

### Use `createDebugLogger` for Everything Else

For UI components, controllers, utilities, and non-exchange code:

```typescript
// React Component
const debug = createDebugLogger({
  prefix: 'OrderForm',
  shouldLogToConsole: () => getDevOverrides().orderFormLogging
});

// Utility Function
const debug = createDebugLogger({
  prefix: 'PriceCalculator',
  enableConsoleLog: true
});

// Web Worker (non-exchange)
const debug = createDebugLogger({
  prefix: 'DataProcessor',
  isProduction: initData.environment.isProduction,
  forceExternalLogging: initData.environment.forceExternalLogging
});
```

### Environment Behavior

Both logging methods now follow the same rules:

| Environment | Severity | External Logging | Console Logging |
|------------|----------|------------------|-----------------|
| Development | Info/Warning | ❌ No | ✅ Yes |
| Development | Error | ✅ Yes | ✅ Yes |
| Development + Force Flag | All | ✅ Yes | ✅ Yes |
| Production | All | ✅ Yes | Configurable |

## Other Logging Functions

The package also exports:

- `log()` - General logging function that updates the log atom
- `grafanaLog()` - Direct Grafana logging without console output
- `logInfo()`, `logWarn()`, `logError()` - Low-level Grafana logging utilities