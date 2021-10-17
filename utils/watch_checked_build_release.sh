#!/bin/bash
#

cargo watch -d 0.2 -w ./Cargo.toml -w ./src/ -s ./utils/check_test_build_release.sh
