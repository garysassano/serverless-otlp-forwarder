import { diag } from '@opentelemetry/api';
import { ExportResult, ExportResultCode } from '@opentelemetry/core';
import { ProtobufTraceSerializer } from '@opentelemetry/otlp-transformer';
import { SpanExporter, ReadableSpan } from '@opentelemetry/sdk-trace-base';
import * as zlib from 'zlib';
import * as fs from 'fs';
import { VERSION } from './version';

const DEFAULT_ENDPOINT = 'http://localhost:4318/v1/traces';
const DEFAULT_COMPRESSION_LEVEL = 6;
const COMPRESSION_LEVEL_ENV_VAR = 'OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL';
const LOG_LEVEL_ENV_VAR = 'OTLP_STDOUT_SPAN_EXPORTER_LOG_LEVEL';
const OUTPUT_TYPE_ENV_VAR = 'OTLP_STDOUT_SPAN_EXPORTER_OUTPUT_TYPE';
const DEFAULT_PIPE_PATH = '/tmp/otlp-stdout-span-exporter.pipe';

/**
 * Log level for the exported spans
 */
export enum LogLevel {
  /**
   * Debug level (most verbose)
   */
  Debug = 'DEBUG',
  
  /**
   * Info level (default)
   */
  Info = 'INFO',
  
  /**
   * Warning level
   */
  Warn = 'WARN',
  
  /**
   * Error level (least verbose)
   */
  Error = 'ERROR'
}

/**
 * Output type for the exporter
 */
export enum OutputType {
  /**
   * Write to stdout (default)
   */
  Stdout = 'stdout',
  
  /**
   * Write to named pipe
   */
  Pipe = 'pipe'
}

/**
 * Configuration options for OTLPStdoutSpanExporter
 */
export interface OTLPStdoutSpanExporterConfig {
  /**
   * GZIP compression level (0-9, where 0 is no compression and 9 is maximum compression).
   * Environment variable OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL takes precedence if set.
   * Defaults to 6 if neither environment variable nor parameter is provided.
   */
  gzipLevel?: number;

  /**
   * Log level for the exported spans.
   * Environment variable OTLP_STDOUT_SPAN_EXPORTER_LOG_LEVEL takes precedence if set.
   * If not provided, no level field will be included in the output.
   */
  logLevel?: LogLevel;

  /**
   * Output type for the exporter (stdout or pipe).
   * Environment variable OTLP_STDOUT_SPAN_EXPORTER_OUTPUT_TYPE takes precedence if set.
   * Defaults to stdout if neither environment variable nor parameter is provided.
   */
  outputType?: OutputType;
}

/**
 * Interface for output handling
 */
interface Output {
  /**
   * Write a line to the output
   * @param line The line to write
   * @param callback Callback to be called when write is complete
   */
  writeLine(line: string, callback: (err?: Error) => void): void;
}

/**
 * Standard output implementation
 */
class StdOutput implements Output {
  writeLine(line: string, callback: (err?: Error) => void): void {
    process.stdout.write(line + '\n', (err) => {
      callback(err);
    });
  }
}

/**
 * Named pipe output implementation
 */
class NamedPipeOutput implements Output {
  public readonly pipePath: string;
  private pipeExists: boolean;

  constructor() {
    this.pipePath = DEFAULT_PIPE_PATH;
    
    // Check if pipe exists once during initialization
    try {
      this.pipeExists = fs.existsSync(this.pipePath);
      if (!this.pipeExists) {
        diag.warn(`Named pipe does not exist: ${this.pipePath}, will fall back to stdout`);
      }
    } catch (e) {
      diag.warn(`Error checking pipe existence: ${e}, will fall back to stdout`);
      this.pipeExists = false;
    }
  }

  writeLine(line: string, callback: (err?: Error) => void): void {
    if (!this.pipeExists) {
      // Fall back to stdout if pipe doesn't exist
      new StdOutput().writeLine(line, callback);
      return;
    }

    // Write to pipe without checking existence again
    fs.writeFile(this.pipePath, line + '\n', (err) => {
      if (err) {
        diag.warn(`Failed to write to pipe: ${err}, falling back to stdout`);
        new StdOutput().writeLine(line, callback);
        return;
      }
      callback();
    });
  }
}

/**
 * Helper function to create output based on type
 */
function createOutput(outputType: OutputType): Output {
  if (outputType === OutputType.Pipe) {
    return new NamedPipeOutput();
  }
  return new StdOutput();
}

/**
 * Parse log level from string
 * @param value The string value to parse
 * @returns The parsed LogLevel or undefined if invalid
 */
function parseLogLevel(value: string): LogLevel | undefined {
  const normalized = value.toLowerCase();
  if (normalized === 'debug') return LogLevel.Debug;
  if (normalized === 'info') return LogLevel.Info;
  if (normalized === 'warn' || normalized === 'warning') return LogLevel.Warn;
  if (normalized === 'error') return LogLevel.Error;
  return undefined;
}

/**
 * An OpenTelemetry span exporter that writes spans to stdout in OTLP format.
 * 
 * This exporter is particularly useful in serverless environments like AWS Lambda
 * where writing to stdout is a common pattern for exporting telemetry data.
 * 
 * Features:
 * - Uses OTLP Protobuf serialization for efficient encoding
 * - Applies GZIP compression with configurable levels
 * - Detects service name from environment variables
 * - Supports custom headers via environment variables
 * - Supports log level for filtering in log aggregation systems
 * - Supports writing to stdout or named pipe
 * 
 * Configuration Precedence:
 * 1. Environment variables (highest precedence)
 * 2. Constructor parameters in config object
 * 3. Default values (lowest precedence)
 * 
 * Environment Variables:
 * - OTEL_SERVICE_NAME: Service name to use in output
 * - AWS_LAMBDA_FUNCTION_NAME: Fallback service name (if OTEL_SERVICE_NAME not set)
 * - OTEL_EXPORTER_OTLP_HEADERS: Global headers for OTLP export
 * - OTEL_EXPORTER_OTLP_TRACES_HEADERS: Trace-specific headers (takes precedence)
 * - OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL: GZIP compression level (0-9). Defaults to 6.
 * - OTLP_STDOUT_SPAN_EXPORTER_LOG_LEVEL: Log level (debug, info, warn, error)
 * - OTLP_STDOUT_SPAN_EXPORTER_OUTPUT_TYPE: Output type (stdout, pipe)
 * 
 * Output Format:
 * ```json
 * {
 *   "__otel_otlp_stdout": "0.1.0",
 *   "source": "my-service",
 *   "endpoint": "http://localhost:4318/v1/traces",
 *   "method": "POST",
 *   "content-type": "application/x-protobuf",
 *   "content-encoding": "gzip",
 *   "headers": {
 *     "tenant-id": "tenant-12345",
 *     "custom-header": "value"
 *   },
 *   "payload": "<base64-encoded-gzipped-protobuf>",
 *   "base64": true,
 *   "level": "INFO"
 * }
 * ```
 * 
 * Example Usage:
 * ```typescript
 * // Basic usage with defaults
 * const exporter = new OTLPStdoutSpanExporter();
 * 
 * // With custom configuration
 * const exporter = new OTLPStdoutSpanExporter({
 *   gzipLevel: 9,
 *   logLevel: LogLevel.Debug,
 *   outputType: OutputType.Pipe
 * });
 * 
 * // With custom headers via environment variables
 * process.env.OTEL_EXPORTER_OTLP_HEADERS = 'tenant-id=tenant-12345,custom-header=value';
 * const exporter = new OTLPStdoutSpanExporter();
 * ```
 */
export class OTLPStdoutSpanExporter implements SpanExporter {
  private readonly endpoint: string;
  private readonly serviceName: string;
  private readonly gzipLevel: number;
  private readonly headers: Record<string, string>;
  private readonly logLevel?: LogLevel;
  private readonly output: Output;

  /**
   * Creates a new OTLPStdoutSpanExporter
   * @param config - Optional configuration options for the exporter
   * @param config.gzipLevel - Optional GZIP compression level (0-9).
   *                    Environment variable OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL takes precedence if set.
   *                    Defaults to 6 if neither environment variable nor parameter is provided.
   * @param config.logLevel - Optional log level for the exported spans.
   *                    Environment variable OTLP_STDOUT_SPAN_EXPORTER_LOG_LEVEL takes precedence if set.
   * @param config.outputType - Optional output type (stdout or pipe).
   *                    Environment variable OTLP_STDOUT_SPAN_EXPORTER_OUTPUT_TYPE takes precedence if set.
   *                    Defaults to stdout if neither environment variable nor parameter is provided.
   */
  constructor(config?: OTLPStdoutSpanExporterConfig) {
    this.endpoint = DEFAULT_ENDPOINT;
    this.serviceName = process.env.OTEL_SERVICE_NAME || 
      process.env.AWS_LAMBDA_FUNCTION_NAME || 
      'unknown-service';
    
    // Set compression level with proper precedence:
    // 1. Environment variable (highest precedence)
    // 2. Constructor parameter from config object
    // 3. Default value (lowest precedence)
    const gzipLevel = config?.gzipLevel;
    const envValue = process.env[COMPRESSION_LEVEL_ENV_VAR];
    if (envValue !== undefined) {
      try {
        const level = parseInt(envValue, 10);
        if (!isNaN(level) && level >= 0 && level <= 9) {
          this.gzipLevel = level;
        } else {
          diag.warn(`Invalid compression level in ${COMPRESSION_LEVEL_ENV_VAR}: ${envValue}, using default level ${DEFAULT_COMPRESSION_LEVEL}`);
          this.gzipLevel = gzipLevel !== undefined ? gzipLevel : DEFAULT_COMPRESSION_LEVEL;
        }
      } catch {
        diag.warn(`Failed to parse ${COMPRESSION_LEVEL_ENV_VAR}: ${envValue}, using default level ${DEFAULT_COMPRESSION_LEVEL}`);
        this.gzipLevel = gzipLevel !== undefined ? gzipLevel : DEFAULT_COMPRESSION_LEVEL;
      }
    } else {
      // No environment variable, use parameter from config or default
      this.gzipLevel = gzipLevel !== undefined ? gzipLevel : DEFAULT_COMPRESSION_LEVEL;
    }
    
    // Set log level with proper precedence:
    // 1. Environment variable (highest precedence)
    // 2. Constructor parameter from config object
    const logLevelEnv = process.env[LOG_LEVEL_ENV_VAR];
    if (logLevelEnv !== undefined) {
      const parsedLogLevel = parseLogLevel(logLevelEnv);
      if (parsedLogLevel !== undefined) {
        this.logLevel = parsedLogLevel;
      } else {
        diag.warn(`Invalid log level in ${LOG_LEVEL_ENV_VAR}: ${logLevelEnv}, log level will not be included in output`);
        this.logLevel = config?.logLevel;
      }
    } else {
      // No environment variable, use parameter from config
      this.logLevel = config?.logLevel;
    }

    // Set output type with proper precedence:
    // 1. Environment variable (highest precedence)
    // 2. Constructor parameter from config object
    // 3. Default value (lowest precedence)
    let outputType = OutputType.Stdout;
    const outputTypeEnv = process.env[OUTPUT_TYPE_ENV_VAR];
    if (outputTypeEnv !== undefined) {
      if (outputTypeEnv.toLowerCase() === 'pipe') {
        outputType = OutputType.Pipe;
      }
    } else if (config?.outputType !== undefined) {
      outputType = config.outputType;
    }
    
    this.output = createOutput(outputType);
    this.headers = this.parseHeaders();
  }

  /**
   * Parse headers from environment variables.
   * Headers should be in the format: key1=value1,key2=value2
   * Filters out content-type and content-encoding as they are fixed.
   * If both OTLP_TRACES_HEADERS and OTLP_HEADERS are defined, merges them with
   * OTLP_TRACES_HEADERS taking precedence.
   * 
   * @returns Record of header key-value pairs
   */
  private parseHeaders(): Record<string, string> {
    return [
      process.env.OTEL_EXPORTER_OTLP_HEADERS,        // General headers first
      process.env.OTEL_EXPORTER_OTLP_TRACES_HEADERS  // Trace-specific headers override
    ]
      .filter((str): str is string => !!str)  // Type guard to filter out undefined
      .reduce((acc, headerStr) => ({
        ...acc,
        ...this.parseHeaderString(headerStr)
      }), {});
  }

  /**
   * Parse a header string in the format key1=value1,key2=value2
   * 
   * @param headerStr - The header string to parse
   * @returns Record of header key-value pairs
   */
  private parseHeaderString(headerStr: string): Record<string, string> {
    return headerStr
      .split(',')
      .map(pair => {
        const [key, ...valueParts] = pair.trim().split('=');
        return [key, valueParts.join('=')];  // Rejoin value parts with '='
      })
      .filter(([key, value]) => key && value && 
        !['content-type', 'content-encoding'].includes(key.toLowerCase().trim()))
      .reduce((acc, [key, value]) => ({
        ...acc,
        [key.trim()]: value.trim()
      }), {});
  }

  /**
   * Exports the spans by serializing them to OTLP Protobuf format, compressing with GZIP,
   * and writing to the configured output as a structured JSON object.
   * 
   * @param spans - The spans to export
   * @param resultCallback - Callback to report the success/failure of the export
   */
  export(spans: ReadableSpan[], resultCallback: (result: ExportResult) => void): void {
    // Check for empty batch and pipe output configuration
    if (spans.length === 0 && this.output instanceof NamedPipeOutput) {
      try {
        // Perform the "pipe touch" operation: write an empty string to the pipe.
        // fs.writeFile handles open/write/close.
        fs.writeFile(this.output.pipePath, '', (err) => {
          if (err) {
            diag.error('Error touching pipe:', err);
            resultCallback({ code: ExportResultCode.FAILED, error: err as Error });
          } else {
            resultCallback({ code: ExportResultCode.SUCCESS });
          }
        });
      } catch (e) {
        // Catch synchronous errors during writeFile setup (unlikely)
        diag.error('Synchronous error during pipe touch setup:', e);
        resultCallback({ code: ExportResultCode.FAILED, error: e as Error });
      }
      return; // Don't proceed with normal export logic
    }

    // Original export logic for non-empty batches or stdout output
    try {
      // Add safety check: If spans somehow became empty after the initial check,
      // or if output is not pipe, do nothing for stdout or return success.
      if (spans.length === 0) {
        return resultCallback({ code: ExportResultCode.SUCCESS });
      }
      
      // Serialize spans to protobuf format using the OTLP transformer
      const serializedData = ProtobufTraceSerializer.serializeRequest(spans);
      if (!serializedData || serializedData.length === 0) {
        // Handle case where serialization yields empty data (e.g., invalid spans)
        diag.warn('ProtobufTraceSerializer.serializeRequest resulted in empty data.');
        return resultCallback({ code: ExportResultCode.SUCCESS }); // Nothing valid to export
      }

      // Compress the serialized data using GZIP with configured compression level
      const compressedData = zlib.gzipSync(serializedData, { level: this.gzipLevel });
      
      // Create the output object with metadata and payload
      const output: Record<string, any> = {
        __otel_otlp_stdout: VERSION,      // Package version for compatibility checking
        source: this.serviceName,          // Service name for identifying the source
        endpoint: this.endpoint,           // Target endpoint (for informational purposes)
        method: 'POST',                    // HTTP method that would be used in OTLP
        'content-type': 'application/x-protobuf',
        'content-encoding': 'gzip',
        payload: compressedData.toString('base64'),  // Base64 encoded GZIP'd protobuf
        base64: true                                 // Indicates payload encoding
      };

      // Add headers section only if there are custom headers
      if (Object.keys(this.headers).length > 0) {
        output.headers = this.headers;
      }

      // Add log level if configured
      if (this.logLevel !== undefined) {
        output.level = this.logLevel;
      }

      // Write the formatted output to the configured output
      this.output.writeLine(JSON.stringify(output), (err) => {
        if (err) {
          diag.error('Failed to write output:', err);
          resultCallback({ code: ExportResultCode.FAILED });
        } else {
          resultCallback({ code: ExportResultCode.SUCCESS });
        }
      });
    } catch (e) {
      diag.error('Error in OTLPStdoutSpanExporter:', e);
      resultCallback({ code: ExportResultCode.FAILED });
    }
  }

  forceFlush(): Promise<void> {
    // Nothing to flush as we write immediately
    return Promise.resolve();
  }

  /**
   * Shutdown the exporter. This implementation is a no-op as stdout doesn't need cleanup.
   * @returns A promise that resolves immediately as there's nothing to clean up
   */
  shutdown(): Promise<void> {
    return Promise.resolve();
  }
}