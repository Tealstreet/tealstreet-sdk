// Vitest setup file for proxy-ips-lists package tests
// This file is run before all tests

// Setup global test environment
if (!process.env.NODE_ENV) {
  process.env.NODE_ENV = 'test';
}
