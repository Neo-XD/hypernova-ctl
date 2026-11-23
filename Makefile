PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
UDEV_RULES_DIR ?= /etc/udev/rules.d
DESTDIR ?=
CARGO ?= cargo

BIN := hypernova-ctl
RULES := 99-cosmic-byte-hypernova.rules
RELEASE_BIN := target/release/$(BIN)

.PHONY: all build install uninstall clean

all: build

build:
	$(CARGO) build --release --locked

install:
	@if [ ! -f "$(RELEASE_BIN)" ]; then \
		echo "Release binary not found at $(RELEASE_BIN). Run 'make' first." >&2; \
		exit 1; \
	fi
	install -Dm755 "$(RELEASE_BIN)" "$(DESTDIR)$(BINDIR)/$(BIN)"
	install -Dm644 "$(RULES)" "$(DESTDIR)$(UDEV_RULES_DIR)/$(RULES)"
	@if [ -z "$(DESTDIR)" ] && command -v udevadm >/dev/null 2>&1; then \
		udevadm control --reload-rules && udevadm trigger; \
	fi

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/$(BIN)"
	rm -f "$(DESTDIR)$(UDEV_RULES_DIR)/$(RULES)"
	@if [ -z "$(DESTDIR)" ] && command -v udevadm >/dev/null 2>&1; then \
		udevadm control --reload-rules && udevadm trigger; \
	fi

clean:
	$(CARGO) clean
