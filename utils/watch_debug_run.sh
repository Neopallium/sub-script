#!/bin/bash
#

cargo watch --no-gitignore -w ./scripts/ -w ./utils/.trip/ -s "./target/debug/run $*"

