# Development images used by compose.dev.yaml. Not used for releases (see Dockerfile).
ARG UID=1000
ARG GID=1000

FROM node:26-alpine AS web
ARG UID
ARG GID
# Pre-create the node_modules mount point owned by the dev user, so the named volume inherits it.
RUN mkdir -p /app/web/node_modules && chown -R ${UID}:${GID} /app
ENV HOME=/tmp
WORKDIR /app/web

FROM rust:1-alpine AS server
ARG UID
ARG GID
RUN apk add --no-cache musl-dev watchexec
# Cargo registry and build output live in named volumes owned by the dev user.
ENV CARGO_HOME=/cargo CARGO_TARGET_DIR=/target HOME=/tmp
RUN mkdir -p /cargo /target /app/server /app/web/build \
 && chown -R ${UID}:${GID} /cargo /target /app
WORKDIR /app/server
