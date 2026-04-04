# syntax=docker/dockerfile:1
ARG BACKEND=tract-backend

FROM rust:1.94-trixie AS builder
ARG BACKEND
# Each backend compiles to its own subdirectory so parallel bake builds
# (inference-tract and inference-ort) never trample each other's incremental cache.
ENV CARGO_TARGET_DIR=/cache/target-${BACKEND}
WORKDIR /app
COPY . .
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    --mount=type=cache,id=edgeflow-inference-target,sharing=shared,target=/cache \
    cargo build --release -p edgeflow-inference --no-default-features --features ${BACKEND} && \
    cp /cache/target-${BACKEND}/release/edgeflow-inference /edgeflow-inference

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /edgeflow-inference /edgeflow-inference
ENV EDGEFLOW_INFER_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/edgeflow-inference"]
