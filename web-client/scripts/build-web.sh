set -e
wasm-pack build --weak-refs --target web --out-name index --out-dir dist/browser/web/main-wasm --no-pack -- --no-default-features  --features primitives
rm dist/browser/web/main-wasm/.gitignore
wasm-pack build --weak-refs --target no-modules --out-name index --out-dir dist/browser/web/worker-wasm --no-pack -- --no-default-features --features client
rm dist/browser/web/worker-wasm/.gitignore
