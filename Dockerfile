FROM rust:1-slim-bookworm AS builder
ARG CARGO_FEATURES=geoip
WORKDIR /app
COPY Cargo.toml ./
COPY src/ src/
COPY templates/ templates/
RUN cargo build --release --features "${CARGO_FEATURES}"

FROM debian:bookworm-slim
ARG UID=1000
ARG GID=1000
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN groupadd --system --gid ${GID} app && useradd --system --no-create-home --uid ${UID} --gid ${GID} app
COPY --from=builder /app/target/release/tally /usr/local/bin/
RUN mkdir -p /data && chown ${UID}:${GID} /data
USER app
WORKDIR /data
ENV PORT=3000 DB_PATH=/data/tally.db RUST_LOG=info,tally=info
EXPOSE 3000
CMD ["tally"]
