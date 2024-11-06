/// <reference types="jest" />

import { createStdoutTransport, StdoutTransportParameters } from '../transport';
import { CompressionAlgorithm } from '@opentelemetry/otlp-exporter-base';
import * as zlib from 'zlib';

jest.mock('zlib', () => ({
  gzip: jest.fn((data: zlib.InputType, callback: zlib.CompressCallback) => {
    callback(null, Buffer.from('mock-compressed-data'));
  }),
  gzipSync: jest.fn(() => Buffer.from('mock-compressed-data')),
  gunzipSync: jest.fn(() => Buffer.from(JSON.stringify({ decompressed: 'mock-decompressed-data' })))
}));

describe('StdoutTransport', () => {
  let mockWrite: jest.SpyInstance;
  let mockLog: jest.SpyInstance;

  beforeAll(() => {
    // Mock console.log to prevent it from writing to process.stdout
    mockLog = jest.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterAll(() => {
    // Restore console.log after all tests
    mockLog.mockRestore();
  });

  beforeEach(() => {
    mockWrite = jest.spyOn(process.stdout, 'write').mockImplementation((
      _str: string | Uint8Array,
      encodingOrCb?: BufferEncoding | ((err?: Error) => void),
      cb?: (err?: Error) => void
    ) => {
      if (typeof encodingOrCb === 'function') {
        encodingOrCb(undefined);
      } else if (typeof cb === 'function') {
        cb(undefined);
      }
      return true;
    });
  });

  afterEach(() => {
    mockWrite.mockRestore();
  });

  const createDefaultParams = (): StdoutTransportParameters => ({
    config: {
      url: 'example.com',
      endpoint: 'example.com'  // Add both for consistency
    },
    contentType: 'application/json',
    headers: {
      'content-type': 'application/json'
    }
  });

  it('should write JSON data to stdout in correct format', async () => {
    const transport = createStdoutTransport(createDefaultParams());
    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    
    const response = await transport.send(testData, 1000);
    
    expect(response.status).toBe('success');
    expect(mockWrite).toHaveBeenCalledTimes(1);
    
    const writtenData = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(writtenData).toMatchObject({
      __otel_otlp_stdout: expect.stringMatching(/^@dev7a\/otlp-stdout-exporter@\d+\.\d+\.\d+$/),
      source: expect.any(String),
      endpoint: 'example.com',
      method: 'POST',
      'content-type': 'application/json',
      base64: false,
      payload: { test: 'data' }
    });
  });

  it('should handle protobuf data correctly', async () => {
    const transport = createStdoutTransport({
      config: {
        url: 'example.com',
        endpoint: 'example.com'  // Add both
      },
      contentType: 'application/x-protobuf',
      headers: {
        'content-type': 'application/x-protobuf'
      }
    });
    
    const testData = Buffer.from('protobuf-data');

    const response = await transport.send(testData, 1000);
    
    expect(response.status).toBe('success');
    expect(mockWrite).toHaveBeenCalledTimes(1);
    
    const writtenData = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(writtenData).toMatchObject({
      __otel_otlp_stdout: expect.any(String),
      source: expect.any(String),
      endpoint: 'example.com',
      method: 'POST',
      'content-type': 'application/x-protobuf',
      base64: true,
      payload: expect.any(String) // Base64 encoded string
    });
  });

  it('should handle gzip compression', async () => {
    const transport = createStdoutTransport({
      config: { 
        compression: CompressionAlgorithm.GZIP,
        url: 'example.com',
        endpoint: 'example.com'  // Add both
      },
      contentType: 'application/json',
      headers: {
        'content-type': 'application/json'
      }
    });
    
    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    
    const response = await transport.send(testData, 1000);
    
    expect(response).toEqual({ status: 'success' });
    expect(mockWrite).toHaveBeenCalledTimes(1);
    
    const writtenData = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(writtenData).toMatchObject({
      'content-encoding': 'gzip',
      base64: true,
      endpoint: 'example.com',
      payload: expect.any(String)
    });
  });

  it('should handle missing configuration gracefully', async () => {
    const params = createDefaultParams();
    const transport = createStdoutTransport(params);
    
    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    const response = await transport.send(testData, 1000);
    
    expect(response.status).toBe('success');
    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.endpoint).toBe('example.com');  // Should match config.endpoint
  });

  it('should handle errors gracefully', async () => {
    mockWrite.mockImplementationOnce((
      _str: string | Uint8Array,
      encodingOrCb?: BufferEncoding | ((err?: Error) => void),
      cb?: (err?: Error) => void
    ) => {
      if (typeof encodingOrCb === 'function') {
        encodingOrCb(new Error('Mock write error'));
      } else if (typeof cb === 'function') {
        cb(new Error('Mock write error'));
      }
      return false;
    });

    const transport = createStdoutTransport(createDefaultParams());
    
    const testData = Buffer.from(JSON.stringify({ test: 'data' }));

    const response = await transport.send(testData, 1000);

    expect(response.status).toBe('failure');

    if (response.status === 'failure') {
      expect(response.error).toBeDefined();
      expect(response.error.message).toBe('Mock write error');
    }
  });
});

describe('StdoutTransport Environment Variables', () => {
  let mockWrite: jest.SpyInstance;
  let originalEnv: NodeJS.ProcessEnv;

  beforeAll(() => {
    // Save original environment
    originalEnv = { ...process.env };
  });

  beforeEach(() => {
    // Clear relevant environment variables before each test
    delete process.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT;
    delete process.env.OTEL_EXPORTER_OTLP_ENDPOINT;
    delete process.env.OTEL_EXPORTER_OTLP_COMPRESSION;
    delete process.env.OTEL_SERVICE_NAME;
    delete process.env.AWS_LAMBDA_FUNCTION_NAME;

    mockWrite = jest.spyOn(process.stdout, 'write').mockImplementation((
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
    mockWrite.mockRestore();
  });

  afterAll(() => {
    // Restore original environment
    process.env = originalEnv;
  });

  it('should prioritize OTEL_EXPORTER_OTLP_TRACES_ENDPOINT over other endpoints', async () => {
    process.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = 'https://traces.example.com';
    process.env.OTEL_EXPORTER_OTLP_ENDPOINT = 'https://general.example.com';
    
    const transport = createStdoutTransport({
      config: {
        url: 'https://config.example.com'
      },
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.endpoint).toBe('https://traces.example.com');
  });

  it('should append /v1/traces to OTEL_EXPORTER_OTLP_ENDPOINT when used', async () => {
    process.env.OTEL_EXPORTER_OTLP_ENDPOINT = 'https://general.example.com';
    
    const transport = createStdoutTransport({
      config: {
        url: 'https://config.example.com'
      },
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.endpoint).toBe('https://general.example.com/v1/traces');
  });

  it('should use config endpoint if no environment variables are set', async () => {
    const transport = createStdoutTransport({
      config: {
        url: 'https://config.example.com'
      },
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.endpoint).toBe('https://config.example.com');
  });

  it('should use default endpoint if no config or environment variables are set', async () => {
    const transport = createStdoutTransport({
      config: {},
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.endpoint).toBe('http://localhost:4318/v1/traces');
  });

  it('should prioritize OTEL_SERVICE_NAME over AWS_LAMBDA_FUNCTION_NAME', async () => {
    process.env.OTEL_SERVICE_NAME = 'otel-service';
    process.env.AWS_LAMBDA_FUNCTION_NAME = 'lambda-function';
    
    const transport = createStdoutTransport({
      config: {},
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.source).toBe('otel-service');
  });

  it('should use AWS_LAMBDA_FUNCTION_NAME if OTEL_SERVICE_NAME is not set', async () => {
    process.env.AWS_LAMBDA_FUNCTION_NAME = 'lambda-function';
    
    const transport = createStdoutTransport({
      config: {},
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output.source).toBe('lambda-function');
  });

  it('should use OTEL_EXPORTER_OTLP_COMPRESSION over config compression', async () => {
    process.env.OTEL_EXPORTER_OTLP_COMPRESSION = 'gzip';
    
    const transport = createStdoutTransport({
      config: {
        compression: CompressionAlgorithm.NONE
      },
      contentType: 'application/json',
      headers: { 'content-type': 'application/json' }
    });

    const testData = Buffer.from(JSON.stringify({ test: 'data' }));
    await transport.send(testData, 1000);

    const output = JSON.parse(mockWrite.mock.calls[0][0] as string);
    expect(output['content-encoding']).toBe('gzip');
    expect(output.base64).toBe(true);
  });
});
