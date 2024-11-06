import { IExporterTransport, ExportResponse } from '@opentelemetry/otlp-exporter-base';
import { OTLPExporterNodeConfigBase, CompressionAlgorithm } from '@opentelemetry/otlp-exporter-base';
import * as zlib from 'zlib';
import { VERSION } from './version';

export interface StdoutTransportParameters {
  config: OTLPExporterNodeConfigBase & {
    endpoint?: string;
  };
  contentType: string;
  headers: Record<string, string>;
}

interface StdoutOutput {
  __otel_otlp_stdout: string;
  source: string;
  endpoint: string;
  method: string;
  headers: Record<string, string>;
  'content-type': string;
  'content-encoding'?: string;
  payload: unknown;
  base64: boolean;
}

export class StdoutTransport implements IExporterTransport {
  private serviceName: string;
  private compression: CompressionAlgorithm;
  private contentType: string;
  private headers: Record<string, string>;

  constructor(private _parameters: StdoutTransportParameters) {
    // Service name from env vars
    this.serviceName = process.env.OTEL_SERVICE_NAME ||
      process.env.AWS_LAMBDA_FUNCTION_NAME ||
      'unknown-service';

    // Compression from env var or config
    this.compression = process.env.OTEL_EXPORTER_OTLP_COMPRESSION ?
      process.env.OTEL_EXPORTER_OTLP_COMPRESSION as CompressionAlgorithm :
      _parameters.config.compression || CompressionAlgorithm.NONE;

    this.contentType = _parameters.contentType;
    this.headers = _parameters.headers;
  }

  private getEndpoint(): string {
    // Follow OTLP endpoint resolution order:
    // 1. Signal-specific endpoint (OTEL_EXPORTER_OTLP_TRACES_ENDPOINT)
    // 2. General endpoint (OTEL_EXPORTER_OTLP_ENDPOINT) with /v1/traces appended
    // 3. Config endpoint
    // 4. Config url
    // 5. Default endpoint
    if (process.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT) {
      return process.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT;
    }

    if (process.env.OTEL_EXPORTER_OTLP_ENDPOINT) {
      return `${process.env.OTEL_EXPORTER_OTLP_ENDPOINT}/v1/traces`;
    }

    if (this._parameters.config.endpoint) {
      return this._parameters.config.endpoint;
    }

    if (this._parameters.config.url) {
      return this._parameters.config.url;
    }

    return 'http://localhost:4318/v1/traces';  // Default OTLP/HTTP endpoint
  }

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  async send(data: Uint8Array, timeoutMillis: number): Promise<ExportResponse> {
    try {
      const output: StdoutOutput = {
        __otel_otlp_stdout: VERSION,
        source: this.serviceName,
        endpoint: this.getEndpoint(),  // Use the helper method
        method: 'POST',
        headers: this.headers,
        'content-type': this.contentType,
        payload: '',
        base64: false
      };

      let isGzip = false;
      const compress = this.compression === CompressionAlgorithm.GZIP;

      // Normalize headers
      const normalizedHeaders = Object.fromEntries(
        Object.entries(this.headers).map(([k, v]) => [k.toLowerCase(), v])
      );
      output.headers = normalizedHeaders;

      // Check for gzip encoding in input
      const contentEncoding = normalizedHeaders['content-encoding']?.toLowerCase();
      if (contentEncoding === 'gzip') {
        output['content-encoding'] = 'gzip';
        isGzip = true;
      }

      // Handle different content types
      if (this.contentType === 'application/json') {
        if (data.length > 0) {
          // Decompress if input is gzipped
          let payload = isGzip
            ? zlib.gunzipSync(data).toString('utf-8')
            : Buffer.from(data).toString('utf-8');
          payload = JSON.parse(payload);

          if (compress) {
            // Compress the JSON string
            const compressedPayload = zlib.gzipSync(JSON.stringify(payload));
            output.payload = compressedPayload.toString('base64');
            output.base64 = true;
            output['content-encoding'] = 'gzip';
          } else {
            output.payload = payload;
            output.base64 = false;
          }
        }
      } else if (this.contentType === 'application/x-protobuf') {
        const payload = data;

        // If input is already gzipped and compression is requested, keep it as is
        if (isGzip && compress) {
          output.payload = Buffer.from(payload).toString('base64');
          output['content-encoding'] = 'gzip';
        }
        // If compression is requested but input is not gzipped
        else if (compress) {
          const compressedPayload = zlib.gzipSync(payload);
          output.payload = Buffer.from(compressedPayload).toString('base64');
          output['content-encoding'] = 'gzip';
        }
        // No compression requested
        else {
          output.payload = Buffer.from(payload).toString('base64');
        }
        output.base64 = true;
      } else {
        throw new Error(`Unsupported content type: ${this.contentType}`);
      }

      return new Promise<ExportResponse>((resolve) => {
        process.stdout.write(JSON.stringify(output) + '\n', err => {
          if (err) {
            resolve({ status: 'failure', error: err });
          } else {
            resolve({ status: 'success' });
          }
        });
      });
    } catch (error) {
      return { status: 'failure', error: error as Error };
    }
  }

  shutdown(): void {
    // Nothing to clean up
  }
}

export function createStdoutTransport(params: StdoutTransportParameters) {
  return new StdoutTransport(params);
}
