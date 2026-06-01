FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml rust-toolchain.toml ./
COPY crates ./crates
COPY scripts ./scripts
RUN cargo build --release -p matchmaker-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/matchmaker-service /usr/local/bin/
ENV MATCHMAKER_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["matchmaker-service"]
