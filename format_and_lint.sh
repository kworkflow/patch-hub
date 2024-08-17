#!/bin/bash

# making bash exit if anything fails
set -e

function format () {
	echo "Formatting..."
	cargo fmt --all
}

# TODO: run rust-clippy

format