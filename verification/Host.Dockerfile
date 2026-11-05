FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6

ENV CARGO_BUILD_JOBS=2
RUN rustup toolchain install nightly-2026-08-21 --profile minimal --component miri,rust-src
RUN cargo install --locked cargo-mutants --version 27.1.0 \
    && cargo install --locked cargo-fuzz --version 0.13.2

WORKDIR /work
