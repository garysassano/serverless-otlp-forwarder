const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');
const { SpanStatusCode } = require('@opentelemetry/api');
const { registerInstrumentations } = require('@opentelemetry/instrumentation');
const { HttpInstrumentation } = require('@opentelemetry/instrumentation-http');
const { AwsInstrumentation } = require('@opentelemetry/instrumentation-aws-sdk');
const { tracer, provider } = initTelemetry('demo-function');

registerInstrumentations({
  tracerProvider: provider,
  instrumentations: [
    new AwsInstrumentation(),
    new HttpInstrumentation()
  ]
});

const axios = require('axios');
const { SQSClient, SendMessageCommand } = require('@aws-sdk/client-sqs');
const sqs = new SQSClient();

const QUOTES_URL = 'https://dummyjson.com/quotes/random';
const QUEUE_URL = process.env.QUOTES_QUEUE_URL;

// Helper function to get random quote from dummyjson
async function getRandomQuote() {
  return tracer.startActiveSpan('getRandomQuote', async (span) => {
    try {
      const response = await axios.get(QUOTES_URL);
      return response.data;
    } catch (error) {
      span.recordException(error);
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw error;
    } finally {
      span.end();
    }
  });
}

// Helper function to send quote to SQS
async function sendQuote(quote) {
  return tracer.startActiveSpan('sendQuote', async (span) => {
    try {
      const command = new SendMessageCommand({
        QueueUrl: QUEUE_URL,
        MessageBody: JSON.stringify(quote),
        MessageAttributes: {
          'quote_id': {
            DataType: 'String',
            StringValue: quote.id.toString()
          },
          'author': {
            DataType: 'String',
            StringValue: quote.author
          }
        }
      });
      await sqs.send(command);
    } catch (error) {
      span.recordException(error);
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw error;
    } finally {
      span.end();
    }
  });
}

// Process a single quote
async function processQuote(span, quoteNumber, totalQuotes) {
  const quote = await getRandomQuote();
  
  span.addEvent('Random Quote Fetched', {
    'log.severity': 'info',
    'log.message': `Successfully fetched random quote ${quoteNumber}/${totalQuotes}`,
    'quote.text': quote.quote,
    'quote.author': quote.author
  });

  await sendQuote(quote);
  
  span.addEvent('Quote Sent', {
    'log.severity': 'info',
    'log.message': `Quote ${quoteNumber}/${totalQuotes} sent to SQS`,
    'quote.id': quote.id
  });

  return quote;
}

// Process a batch of quotes
async function processBatch(span, batchSize) {
  const quotes = [];
  
  for (let i = 0; i < batchSize; i++) {
    const quote = await processQuote(span, i + 1, batchSize);
    quotes.push(quote);
  }
  
  return quotes;
}

// Lambda handler
exports.handler = async (event, context) => {
  return tracedHandler({
    tracer,
    provider,
    name: 'lambda-invocation',
    event,
    context,
    fn: async (span) => {
      try {
        span.addEvent('Lambda Invocation Started', {
          'log.severity': 'info',
          'log.message': 'Lambda function invocation started'
        });

        const batchSize = Math.floor(Math.random() * 10) + 1;
        span.setAttribute('batch.size', batchSize);
        
        const quotes = await processBatch(span, batchSize);

        span.addEvent('Batch Processing Completed', {
          'log.severity': 'info',
          'log.message': `Successfully processed batch of ${batchSize} quotes`
        });

        return {
          statusCode: 200,
          body: JSON.stringify({
            message: `Retrieved and sent ${batchSize} random quotes to SQS`,
            input: event,
            quotes
          })
        };
      } catch (error) {
        span.recordException(error);
        span.setStatus({ code: SpanStatusCode.ERROR });
        throw error;
      }
    }
  });
};
