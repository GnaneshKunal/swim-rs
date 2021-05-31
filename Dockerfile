FROM rust:1.52-slim AS build
WORKDIR /usr/src

RUN rustup target add x86_64-unknown-linux-musl


RUN USER=root cargo new gossip-async
WORKDIR /usr/src/gossip-async
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

COPY src ./src
RUN cargo install --target x86_64-unknown-linux-musl --path .

FROM scratch
COPY --from=build /usr/local/cargo/bin/gossip-async .
USER 1000

ENTRYPOINT ["./gossip-async"]
