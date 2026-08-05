FROM rust:1.89-bookworm AS builder

WORKDIR /src
COPY mona-db /src/mona-db
COPY mona-gateway/Cargo.toml mona-gateway/Cargo.lock /src/mona-gateway/
COPY mona-gateway/src /src/mona-gateway/src

WORKDIR /src/mona-gateway
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 monagateway

COPY --from=builder /src/mona-gateway/target/release/mona-gateway /usr/local/bin/mona-gateway

USER monagateway
WORKDIR /data
ENV GATEWAY_LISTEN=0.0.0.0:27017 \
    HEALTH_LISTEN=0.0.0.0:8080 \
    DATA_DIR=/data \
    LRU_MAX_TENANTS=32

EXPOSE 27017 8080
CMD ["mona-gateway"]
