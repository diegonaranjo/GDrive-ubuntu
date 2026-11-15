# GDrive - Makefile
# Build / install / verify / package shortcuts for the Ubuntu client.

.PHONY: all build install verify uninstall clean check-deb lint help

PACKAGE_NAME = google-drive-ubuntu
# DEBIAN/control is the single source of truth for the version.
VERSION = $(shell grep '^Version:' DEBIAN/control | cut -d' ' -f2)
DEB = build/$(PACKAGE_NAME)-$(VERSION).deb

help: ## Show this help
	@echo "GDrive v$(VERSION)"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

all: build ## Build .deb package

build: ## Build .deb package (compiles the daemon if needed)
	@./build-deb.sh

install: build ## Install the .deb (apt resolves the dependencies)
	@sudo apt-get install -y ./$(DEB)

verify: ## Verify the installation (packaged or setup.sh)
	@./verify.sh

uninstall: ## Remove the package (keeps ~/.config/gdrive)
	@sudo apt-get remove -y $(PACKAGE_NAME)

clean: ## Clean build artifacts
	@rm -rf build/
	@echo "Build directory cleaned"

check-deb: ## Build the package and assert its contents
	@./tools/check-deb.sh

lint: ## Check script syntax (bash -n / sh -n)
	@bash -n install.sh && echo "install.sh: OK"
	@bash -n build-deb.sh && echo "build-deb.sh: OK"
	@bash -n verify.sh && echo "verify.sh: OK"
	@bash -n setup.sh && echo "setup.sh: OK"
	@bash -n launch.sh && echo "launch.sh: OK"
	@bash -n tools/check-deb.sh && echo "tools/check-deb.sh: OK"
	@sh -n DEBIAN/postinst && echo "DEBIAN/postinst: OK"
	@sh -n DEBIAN/postrm && echo "DEBIAN/postrm: OK"
	@echo "All scripts pass syntax check"

package: build check-deb ## Build and verify the package, then show how to install it
	@echo ""
	@echo "Package built: $(DEB)"
	@echo ""
	@echo "To install:"
	@echo "  sudo apt-get install ./$(DEB)"
	@echo ""
	@echo "To verify:"
	@echo "  ./verify.sh"
