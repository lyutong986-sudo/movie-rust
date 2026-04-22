# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS backend-builder

WORKDIR /app/backend

COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /app/backend/target/release/movie-rust-backend /usr/local/bin/movie-rust-backend
COPY frontend/dashboard-ui /app/public

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8096 \
    APP_SERVER_NAME="Movie Rust" \
    APP_STATIC_DIR=/app/public \
    DATABASE_MAX_CONNECTIONS=8

EXPOSE 8096

CMD ["movie-rust-backend"]
