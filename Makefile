# NEARx Build Automation (Explorer web + nearxd)

ifneq (,$(wildcard ./.env))
    include .env
    export
endif

.PHONY: help web web-release dev nearxd nearxd-release clean install-deps repair-js-deps e2e

help:
	@echo "NEARx Build Commands:"
	@echo "  make web            - Build explorer frontend (production)"
	@echo "  make web-release    - Alias for production web build"
	@echo "  make dev            - Start explorer frontend dev server"
	@echo "  make nearxd         - Run nearxd broker daemon"
	@echo "  make nearxd-release - Build nearxd release binary"
	@echo "  make e2e            - Run Selenium E2E suite"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make install-deps   - Install JS dependencies via Yarn Berry"
	@echo "  make repair-js-deps - Reinstall JS deps after cross-platform optional-dep issues"

web:
	@echo "Building explorer frontend..."
	@yarn workspace explorer-frontend build

web-release: web

dev:
	@echo "Starting explorer dev server at http://127.0.0.1:1420"
	@yarn workspace explorer-frontend dev --host 127.0.0.1 --port 1420 --strictPort

nearxd:
	@echo "Starting nearxd broker..."
	@cargo run --bin nearxd

nearxd-release:
	@echo "Building nearxd (release)..."
	@cargo build --release --bin nearxd

e2e:
	@echo "Running Selenium E2E suite..."
	@yarn workspace nearx-e2e test

clean:
	@echo "Cleaning build artifacts..."
	@cargo clean
	@rm -rf web/dist
	@echo "Clean complete"

install-deps:
	@echo "Enabling Corepack and installing workspace dependencies..."
	@corepack enable
	@yarn install
	@echo "Dependencies installed"

repair-js-deps:
	@echo "Repairing JS workspace dependencies..."
	@node -e "const fs=require('fs'); for (const p of ['node_modules','web/node_modules','e2e-tests/node_modules','.yarn/install-state.gz']) fs.rmSync(p,{recursive:true,force:true});"
	@corepack enable
	@yarn install
	@echo "Dependency repair complete"
