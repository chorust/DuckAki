# Derived in part from duckdb/extension-template-rs (commit abb7b2f3f954a47f90fc37a99cda3ce0618e6094)
.PHONY: clean clean_all test_offline build_test_support

PROJ_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

EXTENSION_NAME=duckaki

# Set to 1 to enable Unstable API (binaries will only work on TARGET_DUCKDB_VERSION, forwards compatibility will be broken)
USE_UNSTABLE_C_API=1

# Target DuckDB version
TARGET_DUCKDB_VERSION=v1.5.5
PYTHON_BIN ?= $(shell which python3.11 2>/dev/null || which python3)

all: configure debug

# Include makefiles from DuckDB
include extension-ci-tools/makefiles/c_api_extensions/base.Makefile
include extension-ci-tools/makefiles/c_api_extensions/rust.Makefile

# Pin DuckDB and install pytz for Python TIMESTAMPTZ result conversion.
DUCKDB_PIP_INSTALL = duckdb==1.5.5 pytz

# Ensure extension_version fits 32-byte header field even with no git commits
extension_version:
	mkdir -p configure
	(git rev-parse --short HEAD 2>/dev/null || echo "v0.1.0") > configure/extension_version.txt
	@echo "Writing version $$(cat configure/extension_version.txt) to configure/extension_version.txt"

configure: venv platform extension_version

build_extension_library_debug: check_configure
	DUCKDB_EXTENSION_NAME=$(EXTENSION_NAME) DUCKDB_EXTENSION_MIN_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION) cargo build --locked $(CARGO_OVERRIDE_DUCKDB_RS_FLAG) $(TARGET_INFO)
	$(PYTHON_VENV_BIN) -c "from pathlib import Path;Path('$(EXTENSION_BUILD_PATH)/debug/extension/$(EXTENSION_NAME)').mkdir(parents=True, exist_ok=True)"
	$(PYTHON_VENV_BIN) -c "import shutil;shutil.copyfile('$(TARGET_PATH)/debug$(IS_EXAMPLE)/$(RUST_LIBNAME)', '$(EXTENSION_BUILD_PATH)/debug/$(EXTENSION_LIB_FILENAME)')"

build_extension_library_release: check_configure
	DUCKDB_EXTENSION_NAME=$(EXTENSION_NAME) DUCKDB_EXTENSION_MIN_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION) cargo build --locked $(CARGO_OVERRIDE_DUCKDB_RS_FLAG) --release $(TARGET_INFO)
	$(PYTHON_VENV_BIN) -c "from pathlib import Path;Path('$(EXTENSION_BUILD_PATH)/release/extension/$(EXTENSION_NAME)').mkdir(parents=True, exist_ok=True)"
	$(PYTHON_VENV_BIN) -c "import shutil;shutil.copyfile('$(TARGET_PATH)/release$(IS_EXAMPLE)/$(RUST_LIBNAME)', '$(EXTENSION_BUILD_PATH)/release/$(EXTENSION_LIB_FILENAME)')"

build_test_support: check_configure
	DUCKDB_EXTENSION_NAME=$(EXTENSION_NAME) DUCKDB_EXTENSION_MIN_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION) cargo build --locked --features test-support $(TARGET_INFO)
	$(PYTHON_VENV_BIN) -c "from pathlib import Path;Path('$(EXTENSION_BUILD_PATH)/test_support/extension/$(EXTENSION_NAME)').mkdir(parents=True, exist_ok=True)"
	$(PYTHON_VENV_BIN) -c "import shutil;shutil.copyfile('$(TARGET_PATH)/debug$(IS_EXAMPLE)/$(RUST_LIBNAME)', '$(EXTENSION_BUILD_PATH)/test_support/$(EXTENSION_LIB_FILENAME)')"
	$(PYTHON_VENV_BIN) extension-ci-tools/scripts/append_extension_metadata.py \
			-l $(EXTENSION_BUILD_PATH)/test_support/$(EXTENSION_FILENAME_NO_METADATA) \
			-o $(EXTENSION_BUILD_PATH)/test_support/$(EXTENSION_FILENAME) \
			-n $(EXTENSION_NAME) \
			-dv $(TARGET_DUCKDB_VERSION) \
			-evf configure/extension_version.txt \
			-pf configure/platform.txt $(UNSTABLE_C_API_FLAG)
	$(PYTHON_VENV_BIN) -c "import shutil;shutil.copyfile('$(EXTENSION_BUILD_PATH)/test_support/$(EXTENSION_FILENAME)', '$(EXTENSION_BUILD_PATH)/test_support/extension/$(EXTENSION_NAME)/$(EXTENSION_FILENAME)')"

debug: build_extension_library_debug build_extension_with_metadata_debug
release: build_extension_library_release build_extension_with_metadata_release

test: test_debug
test_debug: test_extension_debug
test_release: test_extension_release

test_offline: build_test_support
	cargo test --locked --features test-support --lib
	$(PYTHON_VENV_BIN) -m duckdb_sqllogictest --test-dir test/sql --external-extension $(PROJ_DIR)build/test_support/$(EXTENSION_FILENAME)
	DUCKAKI_EXTENSION_PATH=$(PROJ_DIR)build/test_support/$(EXTENSION_FILENAME) $(PYTHON_VENV_BIN) -m unittest discover -s tests/integration -p "test_*.py"

clean: clean_build clean_rust
clean_all: clean_configure clean
