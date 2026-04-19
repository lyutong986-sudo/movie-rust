#!/bin/sh
set -eu

movie-rust-backend &
backend_pid="$!"

nginx -g "daemon off;" &
nginx_pid="$!"

shutdown() {
    kill "$backend_pid" "$nginx_pid" 2>/dev/null || true
    wait "$backend_pid" "$nginx_pid" 2>/dev/null || true
}

trap shutdown INT TERM

while true; do
    if ! kill -0 "$backend_pid" 2>/dev/null; then
        wait "$backend_pid"
        exit "$?"
    fi

    if ! kill -0 "$nginx_pid" 2>/dev/null; then
        wait "$nginx_pid"
        exit "$?"
    fi

    sleep 2
done
