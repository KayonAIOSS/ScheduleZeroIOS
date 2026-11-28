# ScheduleZero web: the WebAssembly app + the sync server in one small image.
# Built and published by GitHub Actions as ghcr.io/kayonaioss/schedulezeroweb:latest

# 1. Build the web app (Rust -> WebAssembly) with Trunk.
FROM rust:1-bookworm AS web
RUN rustup target add wasm32-unknown-unknown \
 && curl -fsSL https://github.com/trunk-rs/trunk/releases/latest/download/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xz -C /usr/local/bin
WORKDIR /src
COPY app/ .
# The app lives at https://pciememorycontroller.com/ScheduleZero/
RUN trunk build --release --public-url /ScheduleZero/

# 2. Run it: Python standard library only, same server as debian-server.
FROM python:3.13-alpine
LABEL org.opencontainers.image.source="https://github.com/KayonAIOSS/ScheduleZeroWeb" \
      org.opencontainers.image.description="ScheduleZero web app and sync server" \
      org.opencontainers.image.licenses="NONE"
WORKDIR /app
COPY server/schedulezero_server.py /app/
COPY --from=web /src/dist /app/web
ENV SZ_PORT=8787 SZ_DATA=/data SZ_WEB=/app/web SZ_BASE_PATH=/ScheduleZero PYTHONUNBUFFERED=1
VOLUME /data
EXPOSE 8787
HEALTHCHECK --interval=60s --timeout=5s --start-period=10s \
  CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8787/api/ping', timeout=4)" || exit 1
CMD ["python3", "/app/schedulezero_server.py"]
