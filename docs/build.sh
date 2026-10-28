#!/bin/sh
# SPDX-License-Identifier: MIT
# Builds the documentation into docs/book: the book (mdBook) with the brand fonts and icons, and the API
# documentation of the library under api/.
set -eu
cd "$(dirname "$0")"

mkdir -p src/fonts
cp ../brand/fonts/*.ttf ../brand/fonts/OFL.txt src/fonts/
cp ../brand/web/favicon.svg theme/favicon.svg
cp ../brand/web/icon-192.png theme/favicon.png

mdbook build

cargo doc --no-deps --manifest-path ../Cargo.toml
rm -rf book/api
cp -R ../target/doc book/api
