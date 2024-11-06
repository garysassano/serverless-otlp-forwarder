import { IExporterTransport, ExportResponse } from '@opentelemetry/otlp-exporter-base';
import { OTLPExporterNodeConfigBase } from '@opentelemetry/otlp-exporter-base';
export interface StdoutTransportParameters {
    config: OTLPExporterNodeConfigBase & {
        endpoint?: string;
    };
    contentType: string;
    headers: Record<string, string>;
}
export declare class StdoutTransport implements IExporterTransport {
    private _parameters;
    private serviceName;
    private compression;
    private contentType;
    private headers;
    constructor(_parameters: StdoutTransportParameters);
    private getEndpoint;
    send(data: Uint8Array, timeoutMillis: number): Promise<ExportResponse>;
    shutdown(): void;
}
export declare function createStdoutTransport(params: StdoutTransportParameters): StdoutTransport;
