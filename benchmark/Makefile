.PHONY:  test test-all report clean

# Memory configurations
MEMORY_CONFIGS := 128 512 1024

# Default number of concurrent executions and rounds
CONCURRENCY := 1
ROUNDS := 1
BENCHMARK_CONFIG := benchmark-config.json

# Directory for reports
REPORTS_DIR := reports
RESULT_DIR := results


# Target to run a specific test configuration
test-%:
	@echo "Running test for $* MB memory configurations..."
	@jq -r '.tests[] | "cargo run -- stack benchmark -m $* -c $(CONCURRENCY) -r $(ROUNDS) -s \(.selector) -d results/\(.selector) --payload-file \(.payload) \(.environment | to_entries | map("--env \(.key)=\(.value)") | join(" "))"' $(BENCHMARK_CONFIG) | while read cmd; do \
		echo "Running: $$cmd"; \
		eval $$cmd || exit 1; \
	done

report:
	@echo "Generating report..."
	cargo run -- report --screenshot dark

# Main test target that runs all configurations
test: clean $(patsubst %,test-%,$(MEMORY_CONFIGS))
	@echo "All tests completed. Generating report..."
	cargo run -- report --screenshot dark

# Helper target to show available configurations
help:
	@echo "Available targets:"
	@echo "  clean     - Clean results and reports directory"
	@echo "  test      - Run all tests and generate report"
	@echo "  test-128  - Run tests with 128MB memory"
	@echo "  test-512  - Run tests with 512MB memory"
	@echo "  test-1024 - Run tests with 1024MB memory"
	@echo ""
	@echo "Test configurations:"
	@echo "  Memory:      $(MEMORY_CONFIGS)"
	@echo "  Config:      $(BENCHMARK_CONFIG)"
	@echo "  Concurrency: $(CONCURRENCY)"
	@echo "  Rounds:      $(ROUNDS)"

clean:
	@echo "Cleaning results and reports directory..."
	@rm -rf $(REPORTS_DIR)/*
	@rm -rf $(RESULT_DIR)/*
