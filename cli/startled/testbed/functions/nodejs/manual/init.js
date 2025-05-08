// Required as part of the application to initialize the Lambda internal extension
// needs to be loaded before any other imports using NODE_OPTIONS=--require init.js
require('@dev7a/lambda-otel-lite/extension');
