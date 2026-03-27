# syntax=docker/dockerfile:1
FROM rust:1.94-trixie AS builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    --mount=type=cache,id=edgeflow-inference-target,target=/app/target \
    cargo build --release -p edgeflow-inference && \
    cp /app/target/release/edgeflow-inference /edgeflow-inference

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /edgeflow-inference /edgeflow-inference
ENV EDGEFLOW_INFER_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/edgeflow-inference"]
