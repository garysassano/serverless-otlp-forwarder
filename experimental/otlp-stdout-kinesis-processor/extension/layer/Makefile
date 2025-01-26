# Makefile for building the Rust Lambda Extension layer
# This builds the extension for ARM64 architecture and copies it to the artifacts directory
# The extension is used to capture and process Lambda telemetry events

build-StdoutKinesisOTLPLayer:
	@echo "Building Rust extension layer"
	@cargo lambda build --release --extension --arm64
	@echo "Copying extension layer to artifacts directory"
	@cp -r target/lambda/extensions "$(ARTIFACTS_DIR)"
