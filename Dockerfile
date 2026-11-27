# NOT BUILT OR RUN YET: written from the project layout; the first `docker build` may need fixes.
# Stage 1: the web UI. Stage 2: the server (sqlx macros use the committed .sqlx data).
# Stage 3: a small runtime image. The index, the database and the models are NOT in the image.

FROM node:22-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1-slim AS build
WORKDIR /src
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY migrations migrations
COPY src src
COPY tools/nlp/topics-v3.json tools/nlp/topics-v3.json
RUN cargo build --release --bin rust-knowledge

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd --system --create-home rks
WORKDIR /app
COPY --from=build /src/target/release/rust-knowledge /usr/local/bin/rust-knowledge
COPY --from=web /web/dist /app/web/dist
USER rks
# SEARCH_INDEX_DIR is a volume: a generation is built once and read by every replica.
ENV WEB_DIST=/app/web/dist SEARCH_INDEX_DIR=/data/index
VOLUME ["/data"]
# The API refuses non-loopback bind addresses by design, so the container shares the host
# network namespace (see compose.yaml) instead of publishing a port.
CMD ["rust-knowledge"]
