.PHONY: start-dev build

start-dev:
	./scripts/dev.sh

build:
	cargo build --release
