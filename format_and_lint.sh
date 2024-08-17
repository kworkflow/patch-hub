#!/bin/bash

# making bash exit if anything fails
set -e

CLIPPY_CONFIGS="
--allow=clippy::too-many-arguments \
--deny=warnings \
--deny=clippy::map_unwrap_or \
--deny=unconditional_recursion
"

function format () {
	echo "Formatting..."
	cargo fmt --all
}

function lint () {
	echo "Running clippy..."
	cargo clippy --all-features --all-targets --tests -- $CLIPPY_CONFIGS
}

format && 
lint && 
echo "format_and_lint.sh finished successfully"