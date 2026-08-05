FROM rust:1.89-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 monaedge

COPY --from=builder /src/target/release/mona-edge /usr/local/bin/mona-edge

USER monaedge
WORKDIR /app
ENV EDGE_LISTEN=0.0.0.0:27017 \
    HEALTH_LISTEN=0.0.0.0:8080 \
    CONTROL_PLANE_URL=http://mona-api.mona.svc.cluster.local:8000 \
    TLS_CERT=/certs/tls.crt \
    TLS_KEY=/certs/tls.key

EXPOSE 27017 8080
CMD ["mona-edge"]
