# syntax=docker/dockerfile:1

FROM node:22-alpine AS frontend-builder

WORKDIR /app/frontend

COPY frontend/package*.json ./
RUN npm install

COPY frontend/index.html frontend/tsconfig.json frontend/vite.config.ts ./
COPY frontend/src ./src

RUN npm run build

FROM rust:1-bookworm AS backend-builder

WORKDIR /app/backend

COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates nginx \
    && rm -rf /var/lib/apt/lists/* \
    && rm -f /etc/nginx/sites-enabled/default

WORKDIR /app

COPY --from=backend-builder /app/backend/target/release/movie-rust-backend /usr/local/bin/movie-rust-backend
COPY --from=frontend-builder /app/frontend/dist /usr/share/nginx/html
COPY docker/nginx.conf /etc/nginx/conf.d/movie-rust.conf
COPY docker/entrypoint.sh /usr/local/bin/movie-rust-entrypoint

RUN chmod +x /usr/local/bin/movie-rust-entrypoint

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8096 \
    APP_SERVER_NAME="Movie Rust" \
    DATABASE_MAX_CONNECTIONS=8

EXPOSE 80 8096

CMD ["/usr/local/bin/movie-rust-entrypoint"]
