#!/bin/bash
#

#cargo check && cargo test --lib && cargo build && touch ./scripts/.trip/reload.txt
cargo build --release --bin run && touch ./utils/.trip/reload.txt

