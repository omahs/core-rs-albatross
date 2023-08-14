set -e

cd launcher

# Build for browsers (bundler, web)
yarn tsup --format esm --platform browser --out-dir ../dist/browser *.ts

# Build for node
yarn tsup --format esm,cjs --platform node --out-dir ../dist/node *.ts
