# syntax=docker/dockerfile:1.7

FROM rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff AS chef

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install --locked cargo-chef

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG TARGETPLATFORM
COPY --from=planner /app/recipe.json /app/recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry-${TARGETPLATFORM} \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git-${TARGETPLATFORM} \
    --mount=type=cache,target=/app/target,id=cargo-target-${TARGETPLATFORM} \
    cargo chef cook --release --recipe-path recipe.json --package saku-server

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry-${TARGETPLATFORM} \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git-${TARGETPLATFORM} \
    --mount=type=cache,target=/app/target,id=cargo-target-${TARGETPLATFORM} \
    cargo build -p saku-server --release \
    && cp /app/target/release/saku-server /app/saku-server

FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/saku-server /usr/local/bin/saku-server

RUN mkdir -p /data /etc/saku-server

ENV SAKU__SERVER__HOST=0.0.0.0
ENV SAKU__SERVER__PORT=8080
ENV SAKU__DATABASE__PATH=/data/saku-server.db
ENV RUST_LOG=saku_server=info,tower_http=info

EXPOSE 8080

ENTRYPOINT ["saku-server"]
CMD ["--config", "/etc/saku-server/config.toml"]
