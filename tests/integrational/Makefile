# Integration test Makefile for aprepo
# Usage: make        - run all tests
#        make clean  - remove all generated artifacts

APREPO = ../../target/debug/aprepo
CARGO = cargo

.PHONY: all clean build fixtures test-invalid test-sources test-xapk

all: build fixtures test-invalid test-sources test-xapk test-tokens
	@echo "All integration tests passed."

build:
	$(CARGO) build

clean:
	find . -type d -name cache -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name output -exec rm -rf {} + 2>/dev/null || true
	find . -name state.yaml -delete 2>/dev/null || true
	find . -name '*.log' -delete 2>/dev/null || true
	find fixtures -name '*.apk' -delete 2>/dev/null || true
	find fixtures -name '*.xapk' -delete 2>/dev/null || true
	rm -rf sources/token_direct sources/token_var 2>/dev/null || true

fixtures:
	bash fixtures/generate.sh

# Invalid config tests
test-invalid: build
	@echo "=== Running invalid config tests ==="
	@for dir in invalid/*/; do \
		name=$$(basename "$$dir"); \
		echo "Test invalid: $$name"; \
		cd "$$dir" && \
		if $(APREPO) --config config.yaml download 2>/dev/null; then \
			echo "FAIL: $$name should have failed"; exit 1; \
		else \
			echo "PASS: $$name correctly rejected"; \
		fi; \
		rm -rf cache output state.yaml 2>/dev/null || true; \
		cd - >/dev/null; \
	done

# Token syntax tests
test-tokens: build
	@echo "=== Running token syntax tests ==="
	@# Test with direct token in config
	@mkdir -p sources/token_direct/cache sources/token_direct/output
	@cp fixtures/test.apk sources/token_direct/cache/test.apk
	@echo 'settings:\n  cache_dir: "./cache"\n  output_dir: "./output"\n  retention_depth: 1\n  architectures:\n    - arm64-v8a\nsources:\n  webdl:\n    throttle_policy: dumb\n    throttle_interval: 1s\n    delay_between_requests: 1s\n    packages:\n      - filename: "test.apk"\n        url: "http://127.0.0.1:8768/test.apk"' > sources/token_direct/config.yaml
	@# Test with $VARIABLE syntax (should fail when unset)
	@mkdir -p sources/token_var
	@echo 'settings:\n  cache_dir: "./cache"\n  output_dir: "./output"\n  retention_depth: 1\n  architectures:\n    - arm64-v8a\nsources:\n  webdl:\n    throttle_policy: dumb\n    throttle_interval: 1s\n    delay_between_requests: 1s\n    packages:\n      - filename: "test.apk"\n        url: "$TEST_URL"' > sources/token_var/config.yaml
	@if $(APREPO) --config sources/token_var/config.yaml download 2>/dev/null; then \
		echo "FAIL: token_var should have failed (env var unset)"; exit 1; \
	else \
		echo "PASS: token_var correctly rejected when env var unset"; \
	fi
	@rm -rf sources/token_direct sources/token_var 2>/dev/null || true

# Per-source valid config tests
test-sources: build fixtures
	@echo "=== Running per-source valid config tests ==="
	@for dir in sources/*/; do \
		name=$$(basename "$$dir"); \
		if [ ! -f "$$dir/test.sh" ]; then continue; fi; \
		echo "Test source: $$name"; \
		cd "$$dir" && bash test.sh; \
		ret=$$?; \
		if [ $$ret -ne 0 ]; then \
			echo "FAIL: $$name"; exit 1; \
		fi; \
		rm -rf cache output state.yaml 2>/dev/null || true; \
		cd - >/dev/null; \
	done

# XAPK repack tests
test-xapk: build fixtures
	@echo "=== Running XAPK repack tests ==="
	@for dir in xapk/*/; do \
		name=$$(basename "$$dir"); \
		echo "Test xapk: $$name"; \
		cd "$$dir" && bash test.sh; \
		ret=$$?; \
		if [ $$ret -ne 0 ]; then \
			echo "FAIL: $$name"; exit 1; \
		fi; \
		rm -rf cache output state.yaml 2>/dev/null || true; \
		cd - >/dev/null; \
	done
