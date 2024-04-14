FROM rust:1.77.1 as builder

# create a new empty shell project
RUN USER=root cargo new --bin hlds_exporter
WORKDIR /hlds_exporter

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN --mount=type=cache,target=/home/rust/.cargo/git \
    --mount=type=cache,target=/home/rust/.cargo/registry \
    --mount=type=cache,sharing=private,target=/hlds_exporter/target \
    rustup component add rustfmt clippy \
    && cargo build --release && rm src/*.rs

# copy your source tree
COPY ./src ./src

# build for release
RUN --mount=type=cache,target=/home/rust/.cargo/git \
    --mount=type=cache,target=/home/rust/.cargo/registry \
    --mount=type=cache,sharing=private,target=/hlds_exporter/target \
    rm -rf /hlds_exporter/target/release/tgbot* \
    && touch ./src/main.rs \
    && cargo build --release \
    && cp target/release/hlds_exporter /hlds_exporter/hlds_exporter

FROM gcr.io/distroless/cc-debian12:nonroot as release

# copy the build artifact from the build stage
COPY --from=builder /hlds_exporter/hlds_exporter /

# set the startup command to run your binary
CMD ["/hlds_exporter"]
