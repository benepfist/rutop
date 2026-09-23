# syntax=docker/dockerfile:1
#
# Cross-builds rutop for Linux (static musl) and Windows (mingw) and exports
# the binaries:
#
#   docker build -f docker/Dockerfile --target export --output dist .

FROM rust:1-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends musl-tools gcc-mingw-w64-x86-64 \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl x86_64-pc-windows-gnu

# musl: musl-gcc only compiles C code (ring); linking uses Rust's self-contained
# musl (using musl-gcc as linker mixes two libcs and crashes at runtime).
ENV CC_x86_64_unknown_linux_musl=musl-gcc \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc \
    AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    set -eux; \
    cargo test --locked --target x86_64-unknown-linux-musl; \
    cargo build --locked --release --target x86_64-unknown-linux-musl; \
    cargo build --locked --release --target x86_64-pc-windows-gnu; \
    mkdir -p /out; \
    cp target/x86_64-unknown-linux-musl/release/rutop /out/rutop-linux-x86_64; \
    cp target/x86_64-pc-windows-gnu/release/rutop.exe /out/rutop-windows-x86_64.exe

FROM scratch AS export
COPY --from=builder /out/ /
