FROM node:22-trixie-slim AS ui-builder
WORKDIR /app/ui
COPY apps/ui/package.json apps/ui/package-lock.json ./
RUN npm ci
COPY apps/ui/ ./
RUN npm run build

FROM lukemathwalker/cargo-chef:latest-rust-1.94-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS server-builder
ARG BUILD_PROFILE=release
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    cargo chef cook --profile ${BUILD_PROFILE} --recipe-path recipe.json -p edgeflow-server
COPY . .
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    cargo build --profile ${BUILD_PROFILE} -p edgeflow-server && \
    cp /app/target/${BUILD_PROFILE}/edgeflow-server /edgeflow-server

FROM debian:trixie-slim
COPY --from=server-builder /edgeflow-server /usr/local/bin/edgeflow-server
COPY --from=ui-builder /app/ui/build /static
RUN mkdir -p /data
ENV EDGEFLOW_DATA_DIR=/data
ENV EDGEFLOW_STATIC_DIR=/static
ENV EDGEFLOW_ADDR=0.0.0.0:5000
EXPOSE 5000
CMD ["edgeflow-server"]
