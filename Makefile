.PHONY: release check-version

check-version:
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION is not set. Usage: make release VERSION=x.y.z"; \
		exit 1; \
	fi

release: check-version
	@./scripts/release.sh $(VERSION)

install:
	@echo "Installing Intus via Homebrew..."
	@brew tap-new harryw1/intus 2>/dev/null || true
	@mkdir -p $$(brew --repo harryw1/intus)/Formula
	@cp homebrew/intus.rb $$(brew --repo harryw1/intus)/Formula/
	@brew reinstall harryw1/intus/intus || brew install harryw1/intus/intus
	@echo "Installation complete! Run 'intus --version' to verify."
