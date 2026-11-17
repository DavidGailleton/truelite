# syntax=docker/dockerfile:1
# 1. Build the web app
FROM node:26-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# 2. Build a static server binary with the web app embedded
FROM rust:1-alpine AS server
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY server/ server/
COPY --from=web /src/web/build web/build
RUN cargo build --release --manifest-path server/Cargo.toml

# 3. Minimal runtime image
FROM scratch
COPY --from=server /src/server/target/release/truelite /truelite
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["/truelite"]
