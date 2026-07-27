# Reproducible development environment for the TrustLink escrow contract.
#
# Base image already ships rustup, cargo, rustfmt and clippy for the `vscode`
# user.  On top of it we add:
#   * the wasm build toolchain (wasm32v1-none target + binaryen/wasm-opt)
#   * PostgreSQL 15 (server + client) for the indexer
#   * the Stellar CLI for deploying/invoking on a network
#
# Node.js is provided by the devcontainer `node` feature (see devcontainer.json).
FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

ARG STELLAR_CLI_VERSION=22.8.1

ENV DEBIAN_FRONTEND=noninteractive

# --- OS packages -------------------------------------------------------------
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        binaryen \
        postgresql \
        postgresql-client \
        libpq-dev \
        pkg-config \
        libssl-dev \
        libdbus-1-dev \
        libudev-dev \
        jq \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# --- Stellar CLI (prebuilt binary; optional per CONTRIBUTING.md) --------------
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) target="x86_64-unknown-linux-gnu" ;; \
      arm64) target="aarch64-unknown-linux-gnu" ;; \
      *) target="" ;; \
    esac; \
    if [ -n "$target" ]; then \
      url="https://github.com/stellar/stellar-cli/releases/download/v${STELLAR_CLI_VERSION}/stellar-cli-${STELLAR_CLI_VERSION}-${target}.tar.gz"; \
      if curl -fsSL "$url" -o /tmp/stellar-cli.tar.gz; then \
        tar -xzf /tmp/stellar-cli.tar.gz -C /usr/local/bin stellar; \
        rm -f /tmp/stellar-cli.tar.gz; \
        stellar --version; \
      else \
        echo "WARNING: could not download Stellar CLI ${STELLAR_CLI_VERSION} — install it manually if you need it."; \
      fi; \
    else \
      echo "WARNING: no Stellar CLI release for architecture ${arch}."; \
    fi

# --- Rust toolchain ----------------------------------------------------------
# rust-toolchain.toml pins the channel, components and wasm target; installing
# the target here means the first `cargo build` does not have to download it.
USER vscode
RUN rustup component add rustfmt clippy \
    && rustup target add wasm32v1-none

USER root
