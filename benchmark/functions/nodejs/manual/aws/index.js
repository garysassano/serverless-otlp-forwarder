const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');
const { registerInstrumentations } = require('@opentelemetry/instrumentation');
const { provider, tracer } = initTelemetry('benchmark-aws');

const { AwsInstrumentation } = require('@opentelemetry/instrumentation-aws-sdk');
registerInstrumentations({
    tracerProvider: provider,
    instrumentations: [
      new AwsInstrumentation(),
    ]
  });
  
const { S3Client, ListBucketsCommand } = require('@aws-sdk/client-s3');
const { DynamoDBClient, ListTablesCommand } = require('@aws-sdk/client-dynamodb');

// Initialize clients
const s3Client = new S3Client();
const dynamoClient = new DynamoDBClient();

/**
 * Executes AWS SDK operations (S3 and DynamoDB) with OpenTelemetry instrumentation to generate spans.
 */
async function testAwsOperations() {
    return await tracer.startActiveSpan('aws-operations', async (parentSpan) => {
        try {
            // Test S3 operations
            await tracer.startActiveSpan('s3-operations', async (span) => {
                try {
                    await s3Client.send(new ListBucketsCommand({}));
                    span.setAttribute('aws.operation', 'list_buckets');
                } catch (error) {
                    span.setStatus({ code: SpanStatusCode.ERROR, message: error.message });
                    span.recordException(error);
                    throw error;
                } finally {
                    span.end();
                }
            });

            // Test DynamoDB operations
            await tracer.startActiveSpan('dynamodb-operations', async (span) => {
                try {
                    await dynamoClient.send(new ListTablesCommand({ Limit: 10 }));
                    span.setAttribute('aws.operation', 'list_tables');
                } catch (error) {
                    span.setStatus({ code: SpanStatusCode.ERROR, message: error.message });
                    span.recordException(error);
                    throw error;
                } finally {
                    span.end();
                }
            });
        } finally {
            parentSpan.end();
        }
    });
}

/**
 * Lambda handler that exercises AWS SDK operations with OpenTelemetry instrumentation.
 * 
 * This handler executes S3 and DynamoDB operations to generate telemetry spans. It uses
 * the AwsInstrumentation to automatically instrument AWS SDK calls.
 */
exports.handler = async (event, context) => {

    return await tracedHandler({
        tracer,
        provider,
        name: 'lambda-invocation',
        event,
        context,
        fn: async (span) => {
            await testAwsOperations();
            return {
                statusCode: 200,
                body: JSON.stringify({
                    message: 'Benchmark complete'
                })
            };
        }
    });
}; 