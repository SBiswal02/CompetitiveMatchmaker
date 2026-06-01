.PHONY: build test run simulate docker-build docker-run

build:
	cargo build --release -p matchmaker-service

test:
	cargo test --workspace

run:
	cargo run -p matchmaker-service

simulate:
	python3 scripts/load_simulation.py --players 5000 --concurrency 250

simulate-heavy:
	python3 scripts/load_simulation.py --players 10000 --concurrency 400 --settle-secs 20

docker-build:
	docker build -t matchmaker -f docker/Dockerfile .

docker-run:
	docker run --rm -p 8080:8080 -e RUST_LOG=info matchmaker
