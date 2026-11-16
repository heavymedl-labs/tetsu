# Benchmark image: the harness, its servers, and the oha load generator.
# Build from the repository root: docker build -f benchmarks/Dockerfile -t tetsu-bench .
FROM rust:1.99.1-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS build
RUN apt-get update && apt-get install -y --no-install-recommends make && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked --version 1.16.0 oha
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p tetsu-bench

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
LABEL org.opencontainers.image.source=https://github.com/heavymedl-labs/tetsu
RUN apt-get update && apt-get install -y --no-install-recommends jq && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/oha /usr/local/bin/oha
COPY --from=build /src/target/release/tetsu-bench /usr/local/bin/tetsu-bench
COPY benchmarks/run.sh /usr/local/bin/run-benchmarks
WORKDIR /work
ENTRYPOINT ["run-benchmarks"]
