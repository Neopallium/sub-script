#!/bin/bash
#

#cargo check && cargo test --lib && cargo build && touch ./scripts/.trip/reload.txt
cargo check --release --bin run && cargo build --release && touch ./utils/.trip/reload.txt

