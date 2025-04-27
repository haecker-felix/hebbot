# Build stage

FROM rust:bookworm AS cargo-build
RUN apt-get update && apt-get -y install libolm-dev cmake

WORKDIR /usr/src/hebbot
COPY Cargo.lock .
COPY Cargo.toml .
COPY ./src src

RUN cargo install --locked --path .


# Final stage

FROM debian:stable-slim
RUN apt-get update && apt-get -y install libssl-dev ca-certificates wget curl git libsqlite3-0

COPY --from=cargo-build /usr/local/cargo/bin/hebbot /bin

CMD ["sh", "-c", "RUST_LOG=hebbot=debug hebbot"]
