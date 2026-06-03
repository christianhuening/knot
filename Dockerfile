# syntax=docker/dockerfile:1.7
#
# knot-server multi-arch container.
#   builder #1 (web): node:20 builds the SPA -> /app/web/dist
#   builder #2 (rust): rust:alpine + cargo-zigbuild cross-compiles to
#     ${TARGETARCH}-unknown-linux-musl using the BUILDPLATFORM toolchain
#     (no per-arch QEMU).
#   runtime: scratch + static binary + CA certs + web/dist.

# ----- SPA build -----
FROM --platform=$BUILDPLATFORM node:20-alpine AS web-builder
WORKDIR /app/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ .
RUN pnpm build
# /app/web/dist now contains the built SPA

# ----- Rust build -----
FROM --platform=$BUILDPLATFORM rust:1.90-alpine AS rust-builder
ARG TARGETARCH
RUN apk add --no-cache musl-dev openssl-dev pkgconf clang lld build-base curl xz tar
RUN cargo install cargo-zigbuild --locked
RUN ARCH=$(uname -m) \
 && curl -sSL "https://ziglang.org/download/0.13.0/zig-linux-${ARCH}-0.13.0.tar.xz" \
    | tar -xJ -C /usr/local \
 && ln -s /usr/local/zig-linux-${ARCH}-0.13.0/zig /usr/local/bin/zig
RUN case "$TARGETARCH" in \
      amd64) echo x86_64-unknown-linux-musl > /target ;; \
      arm64) echo aarch64-unknown-linux-musl > /target ;; \
      *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac
RUN rustup target add "$(cat /target)"

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY tools ./tools

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo zigbuild --release --target "$(cat /target)" --bin knot-server \
 && cp "target/$(cat /target)/release/knot-server" /knot-server

# ----- Runtime: scratch -----
FROM scratch AS runtime
COPY --from=rust-builder /knot-server /knot-server
COPY --from=web-builder /app/web/dist /web/dist
# CA bundle for OIDC discovery, OTLP, etc. (musl scratch has no certs).
COPY --from=rust-builder /etc/ssl/cert.pem /etc/ssl/cert.pem
USER 65534:65534
EXPOSE 3000
ENV KNOT_LOG_FORMAT=json
ENV KNOT_WEB_DIST=/web/dist
ENTRYPOINT ["/knot-server"]
