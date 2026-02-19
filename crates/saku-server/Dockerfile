# Stage 1: Build
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release --package saku-server

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/saku-server /usr/local/bin/saku-server

RUN mkdir -p /data

EXPOSE 8080

ENTRYPOINT ["saku-server"]
CMD ["--config", "/etc/saku-server/config.toml"]
