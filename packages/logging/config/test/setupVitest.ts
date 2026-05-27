// Vitest setup file for logging package tests
// This file is run before all tests

// Setup global test environment
import { vi } from 'vitest';

// Mock window if needed in browser environment
if (typeof window === 'undefined') {
  Object.defineProperty(global, 'window', {
    value: {},
    writable: true,
  });
}

// Mock process.env for consistent testing
if (!process.env.NODE_ENV) {
  process.env.NODE_ENV = 'test';
}

// Mock fetch globally
global.fetch = vi.fn();

// Mock console methods to avoid noise in tests unless explicitly testing them
const originalConsole = { ...console };
global.console = {
  ...originalConsole,
  log: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  debug: vi.fn(),
};