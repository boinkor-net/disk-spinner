FROM rust:1-trixie AS builder

WORKDIR /app

# Required for the libudev-sys crate, a dependency of block-utils
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    libudev-dev \
    pkg-config \
 && rm -rf /var/lib/apt/lists/*

# Fetch dependencies into a cacheable layer
COPY Cargo.toml Cargo.lock .cargo ./

RUN mkdir src \
 && echo 'fn main() { println!("dummy"); }' > src/main.rs \
 && cargo build --release --locked \
 && rm -Rf src \
 && rm -Rf target/release/.fingerprint/disk-spinner-*

# Now copy the code and build it
COPY src/ src/

RUN cargo build --release --locked


FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    libudev1 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/disk-spinner /usr/local/bin/disk-spinner

ENTRYPOINT ["/usr/local/bin/disk-spinner"]

CMD ["--help"]
