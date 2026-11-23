PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
UDEV_RULES_DIR ?= /etc/udev/rules.d
SYSTEMD_USER_DIR ?= /usr/lib/systemd/user
DESTDIR ?=
CARGO ?= cargo

BIN := hypernova-ctl
RULES := 70-cosmic-byte-hypernova.rules
SERVICE := packaging/systemd/hypernova-tray.service
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
	install -Dm644 "$(SERVICE)" "$(DESTDIR)$(SYSTEMD_USER_DIR)/hypernova-tray.service"
	sed -i "s|/usr/bin/hypernova-ctl|$(BINDIR)/$(BIN)|g" "$(DESTDIR)$(SYSTEMD_USER_DIR)/hypernova-tray.service"
	@if [ -z "$(DESTDIR)" ] && command -v udevadm >/dev/null 2>&1; then \
		udevadm control --reload-rules && udevadm trigger; \
	fi
	@if [ -z "$(DESTDIR)" ] && command -v systemctl >/dev/null 2>&1; then \
		systemctl --global enable hypernova-tray.service >/dev/null 2>&1 || true; \
		echo "Run 'hypernova-ctl tray on' to start the tray now, or 'hypernova-ctl tray off' to disable it."; \
	fi

uninstall:
	@if [ -z "$(DESTDIR)" ] && command -v systemctl >/dev/null 2>&1; then \
		systemctl --global disable hypernova-tray.service >/dev/null 2>&1 || true; \
	fi
	rm -f "$(DESTDIR)$(BINDIR)/$(BIN)"
	rm -f "$(DESTDIR)$(UDEV_RULES_DIR)/$(RULES)"
	rm -f "$(DESTDIR)$(SYSTEMD_USER_DIR)/hypernova-tray.service"
	@if [ -z "$(DESTDIR)" ] && command -v udevadm >/dev/null 2>&1; then \
		udevadm control --reload-rules && udevadm trigger; \
	fi

clean:
	$(CARGO) clean
