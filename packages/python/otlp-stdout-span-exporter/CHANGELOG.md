# Changelog

All notable changes to this project will be documented in this file.

## [0.1.2] - 2025-01-12

### Fixed
- Fixed packaging configuration to correctly install the package in site-packages

## [0.1.1] - 2025-01-12 [YANKED]

### Added
- Type hints support with py.typed marker

### Note
- This version was yanked due to incorrect packaging configuration that caused the package to be installed in the wrong location

## [0.1.0] - 2025-01-12

### Added
- Initial release
- Support for exporting spans to stdout in OTLP format
- GZIP compression support
- Custom headers support
- Service name detection from environment variables 