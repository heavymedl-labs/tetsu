# The Tetsu image (ADR-0003). Build from the repository root.

# The front-end build (ADR-0036): Vite and Tailwind read the templates, and only the output leaves this stage, so
# the final image has no Node. Pinned to .nvmrc's release, and installed without scripts (ADR-0034).
FROM node:26.13.0-slim@sha256:5adad64f9a1fca2d51972caa65983634c8a774eb300d72b20f7f2a0f60368072 AS web
WORKDIR /src
COPY package.json package-lock.json .npmrc ./
RUN npm ci --ignore-scripts --include=dev --no-audit --no-fund
COPY web web
COPY crates crates
RUN npm run build

FROM rust:1.99.1-slim-bookworm AS build
WORKDIR /src
COPY . .
COPY --from=web /src/crates/tetsu/assets/build crates/tetsu/assets/build
RUN cargo build --release --locked -p tetsu

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
LABEL org.opencontainers.image.source=https://github.com/heavymedl-labs/tetsu
COPY --from=build /src/target/release/tetsu /usr/local/bin/tetsu
ENTRYPOINT ["tetsu"]
