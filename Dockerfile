# syntax=docker/dockerfile:1

#############################################
# Build stage - compiles both binaries
#############################################
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        perl \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install nightly && rustup default nightly \
    && printf '[unstable]\ncodegen-backend = true\n' >> "${CARGO_HOME:-/usr/local/cargo}/config.toml"

WORKDIR /usr/src/flagrant

COPY . .

# `reqwest` (used by flagrant-client, and so by flagrant-cli/flagrant-bombardier) defaults
# to `native-tls`, which dynamically links the system's OpenSSL at runtime - not present in
# a distroless image. `native-tls-vendored` compiles OpenSSL from source and statically
# links it in instead, so the final image needs nothing beyond glibc/libgcc/libstdc++.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/flagrant/target \
    cargo build --release --locked \
        -p flagrant-api -p flagrant-cli \
        --features reqwest/native-tls-vendored \
    && cp target/release/flagrant-api target/release/flagrant-cli /usr/local/bin/

# A writable directory for flagrant-api's SQLite file, pre-created and owned by
# distroless's `nonroot` user (uid/gid 65532) - the final stage has no shell to `mkdir`
# with, and no `chown` binary either.
RUN mkdir -p /data && chown 65532:65532 /data

#############################################
# Runtime image - both binaries, distroless
#############################################
FROM gcr.io/distroless/cc-debian12:nonroot AS final

COPY --from=builder /usr/local/bin/flagrant-api /usr/local/bin/flagrant-api
COPY --from=builder /usr/local/bin/flagrant-cli /usr/local/bin/flagrant-cli
COPY --from=builder --chown=65532:65532 /data /data
COPY --chown=65532:65532 docker/flagrant-api.toml /etc/flagrant/flagrant.toml

ENV DB_NAME=/data/flagrant.db
ENV FLAGRANT_CONFIG=/etc/flagrant/flagrant.toml

EXPOSE 3030
VOLUME ["/data"]

# flagrant-api runs by default (the long-running service); run flagrant-cli against it (or
# any other reachable instance) with, e.g.:
#   docker run --rm -it --entrypoint /usr/local/bin/flagrant-cli flagrant \
#     -h http://<api-host>:3030 -p myproject
ENTRYPOINT ["/usr/local/bin/flagrant-api"]
