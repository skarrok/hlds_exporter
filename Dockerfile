FROM rust:1.77.1 AS chef
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,sharing=private,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,sharing=private,target=/app/target \
    cargo build --release --bin hlds_exporter \
    && cp /app/target/release/hlds_exporter /app

FROM gcr.io/distroless/cc-debian12:nonroot AS release
COPY --from=builder /app/hlds_exporter /
CMD ["/hlds_exporter"]
