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

.PHONY: compose.up
compose.up: ## start dev compose (Postgres) in background
	docker compose -f deploy/compose/dev.yml up -d
	@echo "waiting for Postgres to be healthy..."
	@for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if docker compose -f deploy/compose/dev.yml ps postgres | grep -q "healthy"; then \
			echo "Postgres healthy"; exit 0; \
		fi; sleep 1; \
	done; \
	echo "Postgres did not become healthy in 15s"; exit 1

.PHONY: compose.down
compose.down: ## stop dev compose
	docker compose -f deploy/compose/dev.yml down

.PHONY: compose.logs
compose.logs: ## tail dev compose logs
	docker compose -f deploy/compose/dev.yml logs -f

.PHONY: compose.psql
compose.psql: ## psql into the dev Postgres
	docker compose -f deploy/compose/dev.yml exec postgres psql -U knot -d knot

.PHONY: migrate.up
migrate.up: ## apply pending migrations (against $$DATABASE_URL or compose default)
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate run --source migrations

.PHONY: migrate.down
migrate.down: ## revert the most recent migration
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate revert --source migrations

.PHONY: migrate.info
migrate.info: ## show migration status
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate info --source migrations
