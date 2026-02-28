# Stage 1: Build
FROM rust:1.88-slim AS builder

WORKDIR /app
COPY . .

RUN cargo build --release --package saku-wrk

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/wrk /usr/local/bin/wrk

RUN mkdir -p /data/tdo /data/wrk/logs /workspace

# claude CLI must be provided via volume mount or installed separately.
# To install in-image, uncomment the following (requires adding nodejs):
#   RUN apt-get update && apt-get install -y nodejs npm && rm -rf /var/lib/apt/lists/*
#   RUN npm install -g @anthropic-ai/claude-code

ENV TDO_DATA_DIR=/data/tdo

ENTRYPOINT ["wrk"]
CMD ["daemon", "--dir", "/workspace", "--poll", "60s"]
