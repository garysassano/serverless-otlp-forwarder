/// <reference types="jest" />

/**
 * @fileoverview Tests for StdoutOTLPExporterNode
 */

import { CompressionAlgorithm } from '@opentelemetry/otlp-exporter-base';
import { StdoutOTLPExporterNode } from '../exporter';
import { ReadableSpan } from '@opentelemetry/sdk-trace-base'; // Correct import
import { InstrumentationScope } from '@opentelemetry/core';      // Correct import
import * as zlib from 'zlib';
import { IResource } from '@opentelemetry/resources';

/**
 * Mock implementation of zlib functions to avoid actual compression during tests.
 */
jest.mock('zlib', () => ({
  gzip: jest.fn((data: zlib.InputType, callback: zlib.CompressCallback) => {
    callback(null, Buffer.from(JSON.stringify({ compressed: 'mock-compressed-data' })));
  }),
  gzipSync: jest.fn(() => {
    return Buffer.from(JSON.stringify({ compressed: 'mock-compressed-data' }));
  }),
  gunzipSync: jest.fn(() => {
    return Buffer.from(JSON.stringify({ decompressed: 'mock-decompressed-data' }));
  }),
}));

describe('StdoutOTLPExporterNode', () => {
  let mockStdoutWrite: jest.SpyInstance;
  let mockLog: jest.SpyInstance;

  beforeAll(() => {
    // Mock console.log to prevent actual logging during tests
    mockLog = jest.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterAll(() => {
    // Restore console.log after all tests
    mockLog.mockRestore();
  });

  beforeEach(() => {
    // Mock process.stdout.write to intercept write calls
    mockStdoutWrite = jest.spyOn(process.stdout, 'write').mockImplementation((
      _str: string | Uint8Array,
      encodingOrCb?: BufferEncoding | ((err?: Error) => void),
      cb?: (err?: Error) => void
    ) => {
      if (typeof encodingOrCb === 'function') {
        encodingOrCb();
      } else if (typeof cb === 'function') {
        cb();
      }
      return true;
    });
  });

  afterEach(() => {
    // Restore process.stdout.write after each test
    mockStdoutWrite.mockRestore();
    // Reset environment variables after each test
    delete process.env.OTEL_EXPORTER_OTLP_PROTOCOL;
  });

  /**
   * Creates a mock ReadableSpan instance conforming to the ReadableSpan interface.
   * Ensures all required properties are present.
   *
   * @returns {ReadableSpan} Mocked ReadableSpan object
   */
  const createMockSpan = (): ReadableSpan => ({
    name: 'test-span',
    kind: 0, // SpanKind.INTERNAL
    spanContext: () => ({
      traceId: '1234567890abcdef1234567890abcdef',
      spanId: '1234567890abcdef',
      traceFlags: 1,
      isRemote: false,
      traceState: undefined,
    }),
    parentSpanId: undefined,
    startTime: [1, 1], // [seconds, nanoseconds]
    endTime: [2, 2],    // [seconds, nanoseconds]
    attributes: {},
    events: [],
    links: [],
    status: {
      code: 1, // SpanStatusCode.ERROR
      message: 'Error occurred',
    },
    resource: {
      attributes: {},
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      merge: jest.fn(function (this: IResource, _other: IResource | null): IResource {
        return this;
      }),
    },
    instrumentationLibrary: {
      name: 'test-instrumentation',
      version: '1.0.0',
    } as InstrumentationScope, // Updated property
    droppedAttributesCount: 0,
    droppedEventsCount: 0,
    droppedLinksCount: 0,
    ended: true,
    duration: [1, 1], // HrTime
  });

  /**
   * Tests exporting spans in JSON format.
   */
  it('should export spans in JSON format', async () => {
    // Set the environment variable to use JSON protocol
    process.env.OTEL_EXPORTER_OTLP_PROTOCOL = 'http/json';
    const exporter = new StdoutOTLPExporterNode({
      compression: CompressionAlgorithm.GZIP,
      timeoutMillis: 1000,
      url: 'example.com'  // Add url
    });
          
    const mockSpan = createMockSpan();

    // Define onSuccess and onError mocks
    const onSuccess = jest.fn();
    const onError = jest.fn();

    exporter.send([mockSpan], onSuccess, onError);

    // Wait for asynchronous operations
    await new Promise(resolve => setImmediate(resolve));

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onError).not.toHaveBeenCalled();
    expect(mockStdoutWrite).toHaveBeenCalledTimes(1);
    
    const writtenData = JSON.parse(mockStdoutWrite.mock.calls[0][0] as string);
    expect(writtenData).toMatchObject({
      __otel_otlp_stdout: expect.stringMatching(/^@dev7a\/otlp-stdout-exporter@\d+\.\d+\.\d+$/),
      source: expect.any(String),
      endpoint: 'example.com',
      method: 'POST',
      'content-type': 'application/json',
      'content-encoding': 'gzip',
      base64: true,
      payload: expect.any(String)
    });
  });

  /**
   * Tests handling compression when configured.
   */
  it('should handle compression when configured', (done) => {
    // Set the environment variable to use JSON protocol
    process.env.OTEL_EXPORTER_OTLP_PROTOCOL = 'http/json';
    const exporter = new StdoutOTLPExporterNode({
      compression: CompressionAlgorithm.GZIP,
      timeoutMillis: 5000,
      url: 'example.com'  // Only use url since we're working with OTLPExporterNodeConfigBase
    });
          
    const mockSpan = createMockSpan();
    const onSuccess = jest.fn();
    const onError = jest.fn();

    exporter.send([mockSpan], onSuccess, onError);

    setImmediate(() => {
      try {
        expect(onSuccess).toHaveBeenCalledTimes(1);
        expect(onError).not.toHaveBeenCalled();
        expect(mockStdoutWrite).toHaveBeenCalledTimes(1);

        const writtenData = JSON.parse(mockStdoutWrite.mock.calls[0][0] as string);
        expect(writtenData).toMatchObject({
          __otel_otlp_stdout: expect.any(String),
          source: expect.any(String),
          endpoint: 'example.com',  // The transport should use url as endpoint
          method: 'POST',
          'content-type': 'application/json',
          'content-encoding': 'gzip',
          'base64': true,
          'payload': expect.any(String)
        });
        done();
      } catch (error) {
        done(error);
      }
    });
  });
});
