.PHONY: help test bench bench-quick clean

help:
	@echo "cineyma Actor Framework - Available targets:"
	@echo ""
	@echo "  make test          - run all tests"
	@echo "  make bench         - run all benchmarks (full suite, ~10 min)"
	@echo "  make bench-quick   - run quick benchmark smoke test"
	@echo "  make bench-actor   - run actor spawn benchmarks"
	@echo "  make bench-msg     - run message throughput benchmarks"
	@echo "  make bench-rr      - run request-response benchmarks"
	@echo "  make bench-serial  - run serialization benchmarks"
	@echo "  make bench-gossip  - run cluster gossip benchmarks"
	@echo "  make bench-fail    - run failure detection benchmarks"
	@echo "  make clean         - clean build artifacts"
	@echo ""

test:
	cargo test --all

bench:
	cargo bench

bench-quick:
	cargo bench --bench actor_spawn -- --test

bench-actor:
	cargo bench --bench actor_spawn

bench-msg:
	cargo bench --bench message_throughput

bench-rr:
	cargo bench --bench request_response

bench-serial:
	cargo bench --bench serialization

bench-gossip:
	cargo bench --bench cluster_gossip

bench-fail:
	cargo bench --bench failure_detection

clean:
	cargo clean
