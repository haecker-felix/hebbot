# syntax=docker/dockerfile:1
FROM fedora:35
RUN dnf -y update && dnf -y install rust cargo openssl-devel libolm-devel cmake gcc-c++ && dnf clean all

WORKDIR app
COPY . .

RUN cargo build --release
CMD ["sh", "-c", "RUST_LOG=hebbot=debug target/release/hebbot"]
