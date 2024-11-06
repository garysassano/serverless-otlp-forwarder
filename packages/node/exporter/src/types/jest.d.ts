import type { jest } from '@jest/globals';

declare global {
  const describe: jest.Describe;
  const expect: jest.Expect;
  const it: jest.It;
  const beforeEach: jest.Hook;
  const afterEach: jest.Hook;
  const jest: typeof jest;
}

export {};
