PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
DESTDIR ?=
CARGO ?= cargo
SPOTIFY ?= 1

BIN := target/release/crab

ifeq ($(SPOTIFY),0)
FEATURES := --no-default-features
else
FEATURES :=
endif

.PHONY: all build deps install uninstall test clean help

all: build

help:
	@echo "make                 build target/release/crab"
	@echo "make install         install it to $(BINDIR) (builds first if needed)"
	@echo "make uninstall       remove the installed binary"
	@echo "make test            run the test suite"
	@echo "make clean           remove build output"
	@echo ""
	@echo "Options: SPOTIFY=0 leaves Spotify support out, PREFIX=/usr/local changes"
	@echo "the install location, DESTDIR is honored for packaging."

deps:
	@command -v $(CARGO) >/dev/null 2>&1 || { \
		echo "cargo not found. Install Rust from https://rustup.rs (1.89 or newer)." >&2; \
		exit 1; \
	}
	@if [ "$$(uname -s)" = "Linux" ]; then \
		if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists alsa; then \
			echo "The ALSA development files are missing." >&2; \
			if command -v dnf >/dev/null 2>&1; then \
				echo "Install them with: sudo dnf install alsa-lib-devel pkgconf-pkg-config" >&2; \
			elif command -v apt-get >/dev/null 2>&1; then \
				echo "Install them with: sudo apt install libasound2-dev pkg-config" >&2; \
			elif command -v pacman >/dev/null 2>&1; then \
				echo "Install them with: sudo pacman -S alsa-lib pkgconf" >&2; \
			elif command -v zypper >/dev/null 2>&1; then \
				echo "Install them with: sudo zypper install alsa-devel pkg-config" >&2; \
			else \
				echo "Install the ALSA development package and pkg-config for your distribution." >&2; \
			fi; \
			exit 1; \
		fi; \
	fi

build: deps
	$(CARGO) build --release --locked $(FEATURES)
	@echo "Built $(BIN)"

install:
	@test -x $(BIN) || $(MAKE) build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 $(BIN) $(DESTDIR)$(BINDIR)/crab
	@echo "Installed $(DESTDIR)$(BINDIR)/crab"
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) ;; \
		*) echo "Note: $(BINDIR) is not in your PATH. Add it to your shell profile to run 'crab' from anywhere." ;; \
	esac

uninstall:
	@if [ -e $(DESTDIR)$(BINDIR)/crab ]; then \
		rm -f $(DESTDIR)$(BINDIR)/crab; \
		echo "Removed $(DESTDIR)$(BINDIR)/crab"; \
	else \
		echo "Nothing to remove at $(DESTDIR)$(BINDIR)/crab"; \
	fi
	@echo "Your settings in ~/.config/crab were left in place."

test: deps
	$(CARGO) test --locked $(FEATURES)

clean:
	$(CARGO) clean
