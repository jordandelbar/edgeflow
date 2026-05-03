ARG BACKEND=tract-backend
ARG BUILD_PROFILE=release

FROM lukemathwalker/cargo-chef:latest-rust-1.94-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BACKEND
ARG BUILD_PROFILE
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    cargo chef cook --profile ${BUILD_PROFILE} --recipe-path recipe.json \
        -p edgeflow-inference --no-default-features --features ${BACKEND}
COPY . .
RUN --mount=type=cache,id=cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,sharing=locked,target=/usr/local/cargo/git \
    cargo build --profile ${BUILD_PROFILE} -p edgeflow-inference \
        --no-default-features --features ${BACKEND} && \
    cp /app/target/${BUILD_PROFILE}/edgeflow-inference /edgeflow-inference

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /edgeflow-inference /edgeflow-inference
ENV EDGEFLOW_INFER_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/edgeflow-inference"]
