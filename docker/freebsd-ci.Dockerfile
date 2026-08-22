# CI image for cross-compiling flagrant-api/flagrant-cli to x86_64-unknown-freebsd.
# linux/amd64 only - the upstream cross-rs image has no arm64 build.
FROM ghcr.io/cross-rs/x86_64-unknown-freebsd:main

# actions/checkout is a JS-based action, so `node` must be on PATH before it runs -
# same reasoning as docker/ci.Dockerfile. This base is Ubuntu 20.04 (focal), whose
# apt-packaged nodejs is v10 - too old for actions/checkout@v4 (needs node20) - so
# install a current release from NodeSource instead.
RUN apt-get update && apt-get install -y --no-install-recommends curl unzip ca-certificates \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# This image's apt-installed protobuf-compiler is 3.6.x (Ubuntu focal base), too old
# to understand proto3 `optional` fields (needs 3.15+, ours needs it for features.proto).
# Install a current release directly instead.
RUN curl -sSL -o /tmp/protoc.zip https://github.com/protocolbuffers/protobuf/releases/download/v25.3/protoc-25.3-linux-x86_64.zip \
    && unzip -q /tmp/protoc.zip -d /usr/local \
    && rm /tmp/protoc.zip

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# nightly, to match this workspace's `cargo-features = ["codegen-backend"]`.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly --profile minimal \
    && rustup target add x86_64-unknown-freebsd
