set -e
wasm-pack build --weak-refs --target nodejs --out-name index --out-dir dist/node/node/main-wasm --no-pack -- --no-default-features  --features primitives
rm dist/node/node/main-wasm/.gitignore
wasm-pack build --weak-refs --target nodejs --out-name index --out-dir dist/node/node/worker-wasm --no-pack -- --no-default-features  --features client
rm dist/node/node/worker-wasm/.gitignore
