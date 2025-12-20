.PHONY: release check-version

check-version:
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION is not set. Usage: make release VERSION=x.y.z"; \
		exit 1; \
	fi

release: check-version
	@echo "Releasing version $(VERSION)..."
	
	# 1. Update Cargo.toml
	@sed -i '' 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml
	@cargo check > /dev/null 2>&1 || true # Update Cargo.lock
	
	# 2. Commit Version Bump
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore: bump version to $(VERSION)"
	
	# 3. Tag and Push
	@git tag v$(VERSION)
	@git push origin master
	@git push origin v$(VERSION)
	
	@echo "Waiting for GitHub to generate the tarball..."
	@sleep 5
	
	# 4. Download and Update Formula
	@curl -L -o v$(VERSION).tar.gz https://github.com/harryw1/intus/archive/refs/tags/v$(VERSION).tar.gz
	@START_SHA=$$(shasum -a 256 v$(VERSION).tar.gz | cut -d ' ' -f 1); \
	echo "New SHA256: $$START_SHA"; \
	sed -i '' "s|url \".*\"|url \"https://github.com/harryw1/intus/archive/refs/tags/v$(VERSION).tar.gz\"|" homebrew/intus.rb; \
	sed -i '' "s/sha256 \".*\"/sha256 \"$$START_SHA\"/" homebrew/intus.rb
	@rm v$(VERSION).tar.gz
	
	# 5. Commit Formula Update
	@git add homebrew/intus.rb
	@git commit -m "fix(brew): update formula to v$(VERSION)"
	@git push origin master
	
	@echo "Release $(VERSION) complete!"

install:
	@echo "Installing Intus via Homebrew..."
	@brew tap-new harryw1/intus 2>/dev/null || true
	@mkdir -p $$(brew --repo harryw1/intus)/Formula
	@cp homebrew/intus.rb $$(brew --repo harryw1/intus)/Formula/
	@brew reinstall harryw1/intus/intus || brew install harryw1/intus/intus
	@echo "Installation complete! Run 'intus --version' to verify."
