.DEFAULT_GOAL := help

CARGO := cargo
PNPM := pnpm

.PHONY: help
help: ## show this help
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z0-9._-]+:.*?##/ {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: schema.gen
schema.gen: ## regenerate Rust + TS schema from tools/schema.json
	$(CARGO) run --quiet -p schemagen -- --lang rust --out crates/knot-markdown/src/schema.rs
	$(CARGO) run --quiet -p schemagen -- --lang ts   --out web/src/features/editor/schema.ts

.PHONY: spike.server
spike.server: ## run the spike WebSocket server on :3000
	$(CARGO) run --bin knot-server

.PHONY: spike.web
spike.web: ## run the spike SPA via Vite on :5173 (proxies /collab to :3000)
	cd web && $(PNPM) dev

.PHONY: test
test: test.rust test.web ## run all unit/integration tests

.PHONY: test.rust
test.rust: ## run Rust tests
	$(CARGO) nextest run --workspace --all-features

.PHONY: test.web
test.web: ## run TS unit tests
	cd web && $(PNPM) test

.PHONY: e2e
e2e: ## run the playwright convergence test
	cd e2e && $(PNPM) playwright test

.PHONY: lint
lint: ## cargo clippy + cargo fmt --check + tsc --noEmit
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	$(CARGO) fmt --all -- --check
	cd web && $(PNPM) tsc --noEmit

.PHONY: fmt
fmt: ## cargo fmt + prettier write
	$(CARGO) fmt --all
	cd web && $(PNPM) prettier --write src

.PHONY: clean
clean: ## remove build artifacts
	$(CARGO) clean
	rm -rf web/node_modules web/dist e2e/node_modules
