# The Tetsu image (ADR-0003). Build from the repository root.
FROM rust:1.99.1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p tetsu

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
LABEL org.opencontainers.image.source=https://github.com/heavymedl-labs/tetsu
COPY --from=build /src/target/release/tetsu /usr/local/bin/tetsu
ENTRYPOINT ["tetsu"]
