# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2025-01-18

### Added
- Support for chaining multiple span processors in order
  - Changed `spanProcessor` option to `spanProcessors` array
  - Old: `initTelemetry('name', { spanProcessor: new MyProcessor() })`
  - New: `initTelemetry('name', { spanProcessors: [new MyProcessor()] })`
- Added ability to run examples locally
- Added detailed SAM template examples for async mode configuration
- Added comprehensive environment variables documentation

### Changed
- Updated documentation to use JavaScript examples throughout

### Fixed
- Fixed documentation for processor chaining order

## [0.4.0] - 2025-01-13

### Added
- New handler interface that takes the handler function as a separate parameter for better ergonomics
  ```typescript
  tracedHandler(options, async (span) => { ... })
  ```

### Changed
- Deprecated the old handler interface with `fn` in options object
  - Old interface still works but will show deprecation notice
  - Will be removed in a future major version

### Removed
- Removed flush frequency behavior in async mode
  - Spans are now flushed after every handler completion
  - Removed `LAMBDA_EXTENSION_SPAN_PROCESSOR_FREQUENCY` environment variable

## [0.3.1] - 2025-01-13

### Added
- Export `OTLPStdoutSpanExporter` from main package for easier access
- Improved extension initialization sequence with proper Lambda runtime synchronization

### Changed
- Updated to use `@dev7a/otlp-stdout-span-exporter` instead of `@dev7a/otlp-stdout-exporter`
- Unified logging approach across runtime and extension
- Standardized code formatting and indentation
- Improved error reporting and context in log messages

### Fixed
- Extension initialization and event handling sequence
- Removed unused `processDetectorSync` from resource detectors

## [0.1.1] - 2024-01-05

### Fixed
- Aligned `faas.trigger` attribute behavior with Python implementation:
  - Set default value to 'other' unconditionally
  - Simplified HTTP request detection logic
  - Fixed attribute setting order

## [0.1.0] - 2024-01-05

### Added
- Initial release with basic Lambda instrumentation support
- Automatic FAAS attribute detection
- Support for API Gateway events
- Distributed tracing capabilities
- Three processing modes: sync, async, and finalize 