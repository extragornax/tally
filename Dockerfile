FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src/ src/
COPY templates/ templates/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --system --no-create-home --uid 1000 app
COPY --from=builder /app/target/release/tally /usr/local/bin/
RUN mkdir -p /data && chown app:app /data
USER app
WORKDIR /data
ENV PORT=3000 DB_PATH=/data/tally.db RUST_LOG=info,tally=info
EXPOSE 3000
CMD ["tally"]
