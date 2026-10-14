# mySSH — ciclo de vida. `make help` lista os alvos.

CARGO      ?= cargo
APP        := myssh-app
VERSION    := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
ARCH       := $(shell uname -m | sed 's/arm64/aarch64/')
APP_BUNDLE := target/release/mySSH.app
DMG        := target/release/mySSH_$(VERSION)_$(ARCH).dmg
INSTALL_DIR ?= /Applications

# Servidor SSH de teste (ver testenv/up.sh e AGENTS.md)
TEST_ENV := MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$(CURDIR)/testenv/keys

.DEFAULT_GOAL := help
.PHONY: version help setup build release run run-release fmt fmt-check lint test \
        testenv-up testenv-down test-real check audit icons package-mac \
        install-mac uninstall-mac clean

help: ## Lista os alvos
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

version: ## Mostra a versão atual do workspace
	@echo $(VERSION)

# --- Ambiente -----------------------------------------------------------------

setup: ## Instala ferramentas auxiliares (cargo-packager, cargo-deny)
	$(CARGO) install --locked cargo-packager cargo-deny

# --- Desenvolvimento ----------------------------------------------------------

build: ## Build de debug do workspace
	$(CARGO) build --workspace

release: ## Build de release do app
	$(CARGO) build --release -p $(APP) --locked

run: ## Roda o app (debug)
	$(CARGO) run -p $(APP)

run-release: ## Roda o app (release)
	$(CARGO) run --release -p $(APP)

# --- Qualidade ----------------------------------------------------------------

fmt: ## Formata o código
	$(CARGO) fmt --all

fmt-check: ## Verifica formatação
	$(CARGO) fmt --all --check

lint: ## Clippy com warnings como erro
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test: ## Testes unitários e de UI (sem rede)
	$(CARGO) test --workspace

testenv-up: ## Sobe os servidores SSH de teste (Docker, 127.0.0.1:2222)
	./testenv/up.sh

testenv-down: ## Remove os servidores SSH de teste
	-docker rm -f myssh-testsshd myssh-testsshd-target
	-docker network rm myssh-testnet

test-real: testenv-up ## Testes de integração contra o servidor real (serializados)
	$(TEST_ENV) $(CARGO) test --release --workspace -- --ignored --test-threads=1 \
		--skip real_saida --skip screenshots --skip shots

check: fmt-check lint test ## fmt + clippy + testes (o mesmo que o CI)

audit: ## Auditoria de dependências e licenças (cargo-deny)
	$(CARGO) deny check

# --- Distribuição -------------------------------------------------------------

icons: ## Regenera PNG/ICO/ICNS a partir dos SVGs
	$(CARGO) run -p myssh-icon-tool

package-mac: release ## Gera .app e .dmg (NÃO assinados)
	cd crates/app && $(CARGO) packager --release --formats app,dmg
	@! strings target/release/myssh 2>/dev/null | grep -q MYSSH_DEMO || \
		(echo "ERRO: ganchos de demo no binário de release" && exit 1)
	@echo "Pacotes: $(APP_BUNDLE) e $(DMG)"

install-mac: package-mac ## Instala o .app em /Applications (INSTALL_DIR=...)
	rm -rf "$(INSTALL_DIR)/mySSH.app"
	cp -R "$(APP_BUNDLE)" "$(INSTALL_DIR)/"
	-xattr -dr com.apple.quarantine "$(INSTALL_DIR)/mySSH.app"
	@echo "Instalado em $(INSTALL_DIR)/mySSH.app"

uninstall-mac: ## Remove o .app (os dados do usuário são mantidos)
	rm -rf "$(INSTALL_DIR)/mySSH.app"

clean: ## Remove artefatos de build
	$(CARGO) clean
