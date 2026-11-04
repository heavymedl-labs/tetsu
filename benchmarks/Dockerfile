# Benchmark image: the harness, its servers, and the oha load generator.
# Build from the benchmarks directory: docker build -t tetsu-bench .
FROM rust:1.99.1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends make && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked --version 1.16.0 oha
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source=https://github.com/heavymedl-labs/tetsu
RUN apt-get update && apt-get install -y --no-install-recommends jq && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/oha /usr/local/bin/oha
COPY --from=build /src/target/release/tetsu-bench /usr/local/bin/tetsu-bench
COPY run.sh /usr/local/bin/run-benchmarks
WORKDIR /work
ENTRYPOINT ["run-benchmarks"]
