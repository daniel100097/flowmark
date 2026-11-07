#!/usr/bin/env sh
# Builds the static web app into web/dist: flowmark.wasm + page + viewer. Serve the folder with any
# static file server, e.g.  python3 -m http.server -d web/dist 8080
set -eu
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build -p flowmark_wasm --target wasm32-unknown-unknown --profile wasm

out=web/dist
rm -rf "$out"
mkdir -p "$out"
cp target/wasm32-unknown-unknown/wasm/flowmark_wasm.wasm "$out/flowmark.wasm"
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int "$out/flowmark.wasm" -o "$out/flowmark.wasm"
fi
cp web/index.html web/app.js web/flowmark-wasm.js "$out/"
cp viewer/viewer.js viewer/viewer.css "$out/"
cp examples/register-user.review.ts "$out/example.review.ts"
echo "built $out ($(wc -c < "$out/flowmark.wasm") bytes of wasm)"
