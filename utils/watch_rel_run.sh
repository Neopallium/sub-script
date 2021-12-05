#!/bin/bash
#

cargo watch --no-gitignore -w init_types.json -w ./tests/ -w ./scripts/ -w ./utils/.trip/ -s "./target/release/run $@"

