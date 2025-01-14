export COVERAGE_THRESHOLD=25

.PHONY: start_node rebuild_and_restart logs_tail cqlsh setup_coverage test_with_coverage markdown_coverage_report html_coverage_report check_markdown_coverage_threshold

start_node:
	docker-compose up -d

rebuild_and_restart:
	docker-compose up -d --build l1x-node

logs_tail:
	docker-compose logs -f l1x-node

cqlsh:
	docker-compose exec cassandra cqlsh


l1x_node_bash:
	docker-compose exec l1x-node sh

### code coverage targets

validate_coverage: setup_coverage test_with_coverage markdown_coverage_report check_markdown_coverage_threshold

setup_coverage:
	rm -rf coverage
	rustup component add llvm-tools-preview
	cargo install grcov
	@mkdir -p coverage

test_with_coverage:
	CARGO_INCREMENTAL=0 RUSTFLAGS=-Cinstrument-coverage LLVM_PROFILE_FILE=../coverage/raw/cargo-test-%p-%m.profraw cargo test

markdown_coverage_report:
	@grcov . --binary-path ./target/debug/deps -s . -t markdown --branch --ignore-not-existing --ignore "*/tests" --ignore "*/src/*test*.rs" --ignore "*/src/*mock*.rs" --ignore "*/.cargo/*" --ignore "target/*" -o coverage/REPORT.md

html_coverage_report:
	@grcov . --binary-path ./target/debug/deps -s . -t html --branch --ignore-not-existing --ignore "*/tests" --ignore "*/src/*test*.rs" --ignore "*/src/*mock*.rs" --ignore "*/.cargo/*" --ignore "target/*" -o coverage/html

check_markdown_coverage_threshold:
	@awk -F ': ' -v threshold="$(COVERAGE_THRESHOLD)" '/Total coverage:/ { gsub("%", "", $$NF); if ($$NF < threshold) { print "Insufficient code coverage:", $$NF, "<", threshold; exit 1 } else print "Sufficient code coverage:", $$NF, ">=", threshold }' coverage/REPORT.md
