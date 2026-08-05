FROM rust:1.89-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 monadb

COPY --from=builder /src/target/release/monadb /usr/local/bin/monadb

USER monadb
WORKDIR /data
ENV MONADB_ADDR=0.0.0.0:27017 \
    MONADB_DATA_DIR=/data

EXPOSE 27017
CMD ["monadb"]
