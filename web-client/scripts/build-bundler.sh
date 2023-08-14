set -e
wasm-pack build --weak-refs --target bundler --out-name index --out-dir dist/browser/bundler/main-wasm --no-pack -- --no-default-features  --features primitives
rm dist/browser/bundler/main-wasm/.gitignore
wasm-pack build --weak-refs --target bundler --out-name index --out-dir dist/browser/bundler/worker-wasm --no-pack -- --no-default-features  --features client
rm dist/browser/bundler/worker-wasm/.gitignore
