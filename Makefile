# GDrive - Makefile
# Equivalente al instalador .pkg de macOS

.PHONY: all build install verify uninstall clean help

PACKAGE_NAME = google-drive-ubuntu
VERSION = 1.0.0

help: ## Show this help
	@echo "GDrive v$(VERSION)"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

all: build ## Build .deb package

build: ## Build .deb package
	@./build-deb.sh

install: ## Install directly (without .deb)
	@sudo ./install.sh

verify: ## Verify installation
	@./verify.sh

uninstall: ## Uninstall Google Drive
	@sudo google-drive-uninstall

clean: ## Clean build artifacts
	@rm -rf build/
	@echo "Build directory cleaned"

lint: ## Check script syntax
	@bash -n install.sh && echo "install.sh: OK"
	@bash -n build-deb.sh && echo "build-deb.sh: OK"
	@bash -n verify.sh && echo "verify.sh: OK"
	@echo "All scripts pass syntax check"

test-install: ## Test installation in Docker (requires Docker)
	@docker run --rm -it -v "$(PWD):/workspace" ubuntu:22.04 bash -c \
		"cd /workspace && ./install.sh && ./verify.sh"

package: build ## Build and show package info
	@echo ""
	@echo "Package built: build/$(PACKAGE_NAME)-$(VERSION).deb"
	@echo ""
	@echo "To install:"
	@echo "  sudo dpkg -i build/$(PACKAGE_NAME)-$(VERSION).deb"
	@echo "  sudo apt-get install -f"
	@echo ""
	@echo "To verify:"
	@echo "  ./verify.sh"
