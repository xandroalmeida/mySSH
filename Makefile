# mySSH — ciclo de vida. `make help` lista os alvos.

CARGO      ?= cargo
APP        := myssh-app
VERSION    := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
ARCH       := $(shell uname -m | sed 's/arm64/aarch64/')
APP_BUNDLE := target/release/mySSH.app
DMG        := target/release/mySSH_$(VERSION)_$(ARCH).dmg
INSTALL_DIR ?= /Applications
DEB_ARCH   := $(shell uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
DEB        := target/release/myssh_$(VERSION)_$(DEB_ARCH).deb
APPIMAGE   := target/release/myssh_$(VERSION)_$(ARCH).AppImage
PREFIX     ?= $(HOME)/.local
ICON_SIZES := 16 32 48 64 128 256 512

# Servidor SSH de teste (ver testenv/up.sh e AGENTS.md)
TEST_ENV := MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$(CURDIR)/testenv/keys

.DEFAULT_GOAL := help
.PHONY: version help setup build release run run-release fmt fmt-check lint test \
        testenv-up testenv-down test-real check audit icons package-mac \
        install-mac uninstall-mac deps-linux package-linux install-linux \
        uninstall-linux install-deb clean

help: ## Lista os alvos
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

version: ## Mostra a versão atual do workspace
	@echo $(VERSION)

# --- Ambiente -----------------------------------------------------------------

setup: ## Instala ferramentas auxiliares (cargo-packager, cargo-deny)
	$(CARGO) install --locked cargo-packager cargo-deny

deps-linux: ## Instala as bibliotecas de sistema (Debian/Ubuntu, sudo; o mesmo que a CI)
	sudo apt-get install -y --no-install-recommends \
		pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
		libx11-dev libxcb1-dev libxcursor-dev libxrandr-dev libxi-dev \
		libgl1-mesa-dev libvulkan1 mesa-vulkan-drivers

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

package-linux: release ## Gera .deb e .AppImage (NÃO assinados)
	cd crates/app && $(CARGO) packager --release --formats deb,appimage
	@! strings target/release/myssh 2>/dev/null | grep -q MYSSH_DEMO || \
		(echo "ERRO: ganchos de demo no binário de release" && exit 1)
	@echo "Pacotes: $(DEB) e $(APPIMAGE)"

install-linux: release ## Instala binário, ícones e .desktop em PREFIX (padrão ~/.local, sem sudo)
	install -Dm755 target/release/myssh "$(PREFIX)/bin/myssh"
	for n in $(ICON_SIZES); do \
		install -Dm644 crates/app/assets/icon-$$n.png \
			"$(PREFIX)/share/icons/hicolor/$${n}x$${n}/apps/myssh.png"; \
	done
	install -Dm644 crates/app/assets/icon.svg \
		"$(PREFIX)/share/icons/hicolor/scalable/apps/myssh.svg"
	install -d "$(PREFIX)/share/applications"
	sed 's|^Exec=.*|Exec=$(PREFIX)/bin/myssh|' crates/app/assets/myssh.desktop \
		> "$(PREFIX)/share/applications/myssh.desktop"
	-gtk-update-icon-cache -q -t -f "$(PREFIX)/share/icons/hicolor" 2>/dev/null
	-update-desktop-database -q "$(PREFIX)/share/applications" 2>/dev/null
	@echo "Instalado em $(PREFIX)/bin/myssh"

uninstall-linux: ## Remove a instalação de PREFIX (os dados do usuário são mantidos)
	rm -f "$(PREFIX)/bin/myssh" "$(PREFIX)/share/applications/myssh.desktop"
	rm -f "$(PREFIX)/share/icons/hicolor/scalable/apps/myssh.svg"
	for n in $(ICON_SIZES); do \
		rm -f "$(PREFIX)/share/icons/hicolor/$${n}x$${n}/apps/myssh.png"; \
	done
	-update-desktop-database -q "$(PREFIX)/share/applications" 2>/dev/null

install-deb: package-linux ## Instala o .deb no sistema (sudo apt)
	sudo apt-get install -y "./$(DEB)"

clean: ## Remove artefatos de build
	$(CARGO) clean
