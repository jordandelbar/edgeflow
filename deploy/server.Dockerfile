# syntax=docker/dockerfile:1
FROM node:22-trixie-slim AS ui-builder
WORKDIR /app/ui
COPY apps/ui/package.json apps/ui/package-lock.json ./
RUN npm ci
COPY apps/ui/ ./
RUN npm run build

FROM rust:1.94-trixie AS server-builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    --mount=type=cache,id=edgeflow-server-target,target=/app/target \
    cargo build --release -p edgeflow-server && \
    cp /app/target/release/edgeflow-server /edgeflow-server

FROM debian:trixie-slim
COPY --from=server-builder /edgeflow-server /usr/local/bin/edgeflow-server
COPY --from=ui-builder /app/ui/build /static
RUN mkdir -p /data
ENV EDGEFLOW_DATA_DIR=/data
ENV EDGEFLOW_STATIC_DIR=/static
ENV EDGEFLOW_ADDR=0.0.0.0:5000
EXPOSE 5000
CMD ["edgeflow-server"]
