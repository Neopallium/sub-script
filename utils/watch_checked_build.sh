#!/bin/bash
#

#cargo watch -d 0.2 -i ./scripts/ -i ./utils/ -s ./utils/check_test_build_debug.sh
cargo watch -d 0.2 -w ./Cargo.toml -w ./src/ -s ./utils/check_test_build_debug.sh
