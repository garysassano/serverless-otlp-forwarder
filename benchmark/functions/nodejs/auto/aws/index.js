const { trace, SpanStatusCode } = require('@opentelemetry/api');
const { S3Client, ListBucketsCommand } = require('@aws-sdk/client-s3');
const { DynamoDBClient, ListTablesCommand } = require('@aws-sdk/client-dynamodb');

// Initialize clients
const s3Client = new S3Client();
const dynamoClient = new DynamoDBClient();

const tracer = trace.getTracer('benchmark-test');

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
    await testAwsOperations();
    
    return {
        statusCode: 200,
        body: JSON.stringify({
            message: 'Benchmark complete'
        })
    };
}; 