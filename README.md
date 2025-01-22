# youtube-cookie-refresher
To cross compile from macOS:
```
rustup target add x86_64-unknown-linux-musl
brew install filosottile/musl-cross/musl-cross
CC=x86_64-linux-musl-cc cargo build --release --target x86_64-unknown-linux-musl --config target.x86_64-unknown-linux-musl.linker=\"x86_64-linux-musl-ld\"
```
