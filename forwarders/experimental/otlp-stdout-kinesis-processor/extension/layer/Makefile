# Makefile for building the Rust Lambda Extension layer
# This builds the extension for ARM64 architecture and copies it to the artifacts directory
# The extension is used to capture and process Lambda telemetry events


# Define a reusable function for building the extension layer
define build-extension-layer
	@echo "Building Rust extension layer for $(1)"
	@cargo lambda build --release --extension --$(2)
	@echo "Copying extension layer to artifacts directory"
	@mkdir -p "$(ARTIFACTS_DIR)/extensions"
	@cp "$(shell cargo metadata --format-version=1 | jq -r '.target_directory')/lambda/extensions/otlp-stdout-kinesis-extension-layer" "$(ARTIFACTS_DIR)/extensions/otlp-stdout-kinesis-extension-layer-$(3)"
	@chmod +x "$(ARTIFACTS_DIR)/extensions/otlp-stdout-kinesis-extension-layer-$(3)"
endef

build-ExtensionLayerARM64:
	$(call build-extension-layer,ARM64,arm64,arm64)

build-ExtensionLayerAMD64:
	$(call build-extension-layer,AMD64,x86-64,amd64)