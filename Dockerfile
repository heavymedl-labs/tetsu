# The Tetsu image (ADR-0003). Build from the repository root.

# The front-end build (ADR-0036): Vite and Tailwind read the templates, and only the output leaves this stage, so
# the final image has no Node. Pinned to .nvmrc's release, and installed without scripts (ADR-0034).
FROM node:26.13.0-slim@sha256:5adad64f9a1fca2d51972caa65983634c8a774eb300d72b20f7f2a0f60368072 AS web
WORKDIR /src
COPY package.json package-lock.json .npmrc ./
RUN npm ci --ignore-scripts --include=dev --no-audit --no-fund
COPY web web
COPY crates crates
COPY docs/brand docs/brand
RUN npm run build

FROM rust:1.99.1-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS build
WORKDIR /src
# Dependencies first, built from the manifests alone over stub sources, so this layer, and the build cache CI
# keeps of it, last until a manifest or the lockfile changes. A new crate needs its manifest copied here.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/audit/Cargo.toml crates/audit/
COPY crates/identity/Cargo.toml crates/identity/
COPY crates/kernel/Cargo.toml crates/kernel/
COPY crates/postgres/Cargo.toml crates/postgres/
COPY crates/tenancy/Cargo.toml crates/tenancy/
COPY crates/tetsu/Cargo.toml crates/tetsu/
COPY benchmarks/Cargo.toml benchmarks/
COPY benchmarks/cornucopia/Cargo.toml benchmarks/cornucopia/
RUN for package in crates/* benchmarks/cornucopia; do mkdir -p "$package/src" && touch "$package/src/lib.rs"; done \
    && rm crates/tetsu/src/lib.rs \
    && for package in crates/tetsu benchmarks; do \
        mkdir -p "$package/src" && echo 'fn main() {}' >"$package/src/main.rs"; \
    done \
    && cargo build --release --locked -p tetsu \
    && rm -r crates/*/src benchmarks/src benchmarks/cornucopia/src
COPY . .
COPY --from=web /src/crates/tetsu/assets/build crates/tetsu/assets/build
# Tetsu's own crates were built from stubs above; cleaning them makes sure only their real sources reach the binary.
RUN cargo clean --release -p tetsu -p tetsu-audit -p tetsu-identity -p tetsu-kernel -p tetsu-postgres -p tetsu-tenancy \
    && cargo build --release --locked -p tetsu

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
LABEL org.opencontainers.image.source=https://github.com/heavymedl-labs/tetsu
COPY --from=build /src/target/release/tetsu /usr/local/bin/tetsu
ENTRYPOINT ["tetsu"]
