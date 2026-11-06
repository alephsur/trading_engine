FROM node:24-bookworm-slim AS dashboard
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
RUN npm run build
FROM rust:1.96-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --locked --release -p trading-server
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/trading-server /usr/local/bin/trading-server
COPY --from=dashboard /web/dist /app/dashboard
ENV DASHBOARD_DIR=/app/dashboard
USER 10001:10001
EXPOSE 3000
ENTRYPOINT ["trading-server"]
