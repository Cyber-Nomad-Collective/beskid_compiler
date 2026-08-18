# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the Beskid pckg Rust backend (Axum).
# Binary: beskid_pckg_server (from compiler/crates/beskid_pckg_server).
#
# Build context is the compiler/ workspace, but this Dockerfile deliberately
# builds a TRIMMED workspace containing only the six beskid_pckg_* crates.
# The full compiler workspace has path dependencies on the sibling
# beskid_bsol submodule (../beskid_bsol) and vendor/ crates (cranelift, ratkit)
# that are not present on the deployment server and are not in the pckg
# dependency closure. A trimmed workspace manifest avoids resolving those
# members entirely, so the build is self-contained and fast.
#
# SQLx uses runtime-tokio-rustls (pure-Rust TLS via ring); no system OpenSSL
# or libpq is required at build or runtime. ring needs a C compiler + make,
# both present in the rust:1-bookworm image.

# --- Build stage -------------------------------------------------------------
FROM rust:1-bookworm AS build

# ring (rustls crypto backend) needs a C compiler and make; both ship in the
# rust:1-bookworm image. No extra -dev packages are required.
ENV CARGO_TERM_COLOR=always
# Keep cargo off the network for the source-copy steps; deps come from the
# registry only during the explicit fetch/build steps.
WORKDIR /app

# Trimmed workspace manifest — only the pckg crates. This is generated in the
# image so the build never touches the rest of the compiler workspace (which
# would require the absent beskid_bsol submodule).
RUN cat > Cargo.toml <<'EOF'
[workspace]
resolver = "3"
members = [
  "crates/beskid_pckg_artifacts",
  "crates/beskid_pckg_auth",
  "crates/beskid_pckg_contract",
  "crates/beskid_pckg_operations",
  "crates/beskid_pckg_server",
  "crates/beskid_pckg_store",
]

[profile.release]
codegen-units = 16
debug = "line-tables-only"
strip = "symbols"
panic = "abort"
EOF

# Copy the lockfile and every pckg manifest first so the dependency layer is
# cached independently of source changes.
COPY Cargo.lock ./
COPY crates/beskid_pckg_artifacts/Cargo.toml ./crates/beskid_pckg_artifacts/
COPY crates/beskid_pckg_auth/Cargo.toml     ./crates/beskid_pckg_auth/
COPY crates/beskid_pckg_contract/Cargo.toml ./crates/beskid_pckg_contract/
COPY crates/beskid_pckg_operations/Cargo.toml ./crates/beskid_pckg_operations/
COPY crates/beskid_pckg_server/Cargo.toml  ./crates/beskid_pckg_server/
COPY crates/beskid_pckg_store/Cargo.toml    ./crates/beskid_pckg_store/

# Create stub sources so cargo can compile the dependency closure and cache it
# in a layer below the real source. beskid_pckg_server is both a lib and a bin.
RUN for c in beskid_pckg_artifacts beskid_pckg_auth beskid_pckg_contract \
             beskid_pckg_operations beskid_pckg_store; do \
        mkdir -p "crates/$c/src" && : > "crates/$c/src/lib.rs"; \
    done && \
    mkdir -p crates/beskid_pckg_server/src && \
    : > crates/beskid_pckg_server/src/lib.rs && \
    printf 'fn main() {}\n' > crates/beskid_pckg_server/src/main.rs

# Build the dependency closure once. This layer is reused across source-only
# changes. The stub binary compiles cleanly; the real source overwrites it next.
RUN cargo build --release -p beskid_pckg_server

# Copy the real sources (and the embedded SQL migrations for beskid_pckg_store,
# which are include_str!'d at compile time) and build the actual binary.
COPY crates/beskid_pckg_artifacts/src ./crates/beskid_pckg_artifacts/src
COPY crates/beskid_pckg_auth/src     ./crates/beskid_pckg_auth/src
COPY crates/beskid_pckg_contract/src ./crates/beskid_pckg_contract/src
COPY crates/beskid_pckg_operations/src ./crates/beskid_pckg_operations/src
COPY crates/beskid_pckg_store/src        ./crates/beskid_pckg_store/src
COPY crates/beskid_pckg_store/migrations ./crates/beskid_pckg_store/migrations
COPY crates/beskid_pckg_server/src      ./crates/beskid_pckg_server/src

# Touch the stubs' timestamps so cargo notices the source change and recompiles
# the pckg crates (reusing the cached dependency artifacts).
RUN find crates -name '*.rs' -exec touch {} + && \
    cargo build --release -p beskid_pckg_server

# --- Runtime stage -----------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# No system TLS/Postgres libs needed: SQLx uses rustls (ring) and a pure-Rust
# Postgres protocol. wget is only for the healthcheck.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /app/target/release/beskid_pckg_server /app/beskid_pckg_server

# Default bind; overridden by compose if needed. pckg-api listens on 8083 to
# avoid colliding with the pckg-web Nitro default (8082).
ENV PCKG_BIND_ADDRESS=0.0.0.0:8083
ENV RUST_LOG=info

EXPOSE 8083

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD wget -q --spider http://127.0.0.1:8083/health || exit 1

CMD ["/app/beskid_pckg_server"]
