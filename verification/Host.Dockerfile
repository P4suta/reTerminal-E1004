FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c

ENV CARGO_BUILD_JOBS=2
RUN rustup toolchain install nightly-2026-08-21 --profile minimal --component miri,rust-src
RUN cargo install --locked cargo-mutants --version 27.1.0 \
    && cargo install --locked cargo-fuzz --version 0.13.2

WORKDIR /work
