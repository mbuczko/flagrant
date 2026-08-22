# CI image for the `test` job in .gitea/workflows/dockerize.yml - bundles the
# system deps that job otherwise installs (and re-downloads) on every run.
FROM rustlang/rust:nightly

RUN apt-get update && apt-get install -y --no-install-recommends \
        nodejs \
        protobuf-compiler \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustc-codegen-cranelift
