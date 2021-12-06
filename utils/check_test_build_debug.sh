#!/bin/bash
#

#cargo check && cargo test --lib && cargo build && touch ./utils/.trip/reload.txt
cargo build --bin run && touch ./utils/.trip/reload.txt

