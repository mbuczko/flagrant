# syntax=docker/dockerfile:1

#############################################
# Build stage - compiles both binaries
#############################################
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        musl-tools \
        perl \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# `distroless/static` has no libc at all, so we target musl (statically linked) instead of
# the default glibc host triple.
RUN rustup toolchain install nightly && rustup default nightly \
    && rustup component add rust-src \
    && HOST="$(rustc --print host-tuple)" \
    && rustup target add "${HOST%-gnu}-musl"

WORKDIR /usr/src/flagrant

COPY . .

# `reqwest` (used by flagrant-client, and so by flagrant-cli/flagrant-bombardier) defaults
# to `native-tls`, which dynamically links the system's OpenSSL at runtime - not present in
# a distroless image. `native-tls-vendored` compiles OpenSSL from source and statically
# links it in instead, so the final image needs nothing beyond glibc/libgcc/libstdc++.
# `--target` is required even though we're not cross-compiling: without it, Cargo doesn't
# treat this as a cross-compile and leaks RUSTFLAGS (notably -Cpanic=immediate-abort) into
# build scripts and proc-macro crates, which rustc always compiles with panic=unwind - causing
# a "core was compiled with a panic strategy which is incompatible" error.
# See https://github.com/rust-lang/rust/issues/146974
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/flagrant/target \
    HOST="$(rustc --print host-tuple)" && \
    TARGET="${HOST%-gnu}-musl" && \
    CC_aarch64_unknown_linux_musl=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc \
    RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none -Zunstable-options -Cpanic=immediate-abort" \
    cargo build --release --locked \
      -Z build-std=std,panic_abort \
      -Z build-std-features= \
      --target "$TARGET" \
      -p flagrant-api -p flagrant-cli --features reqwest/native-tls-vendored \
    && cp "target/$TARGET/release/flagrant-api" "target/$TARGET/release/flagrant-cli" /usr/local/bin/

# A writable directory for flagrant-api's SQLite file, pre-created and owned by
# distroless's `nonroot` user (uid/gid 65532) - the final stage has no shell to `mkdir`
# with, and no `chown` binary either.
RUN mkdir -p /data && chown 65532:65532 /data

#############################################
# Runtime image - both binaries, distroless
#############################################
FROM gcr.io/distroless/static-debian12:nonroot AS final

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
