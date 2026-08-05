FROM rust:1.89-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 monaapi

COPY --from=builder /src/target/release/mona-api /usr/local/bin/mona-api
COPY templates /app/templates

USER monaapi
WORKDIR /app
ENV TEMPLATES_DIR=/app/templates \
    LISTEN_ADDR=0.0.0.0:8000

EXPOSE 8000
CMD ["mona-api"]
