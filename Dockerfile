FROM rust:alpine as builder

RUN apk add --no-cache \
    musl-dev \
    libsfml-dev \
    build-base \
    && rustup target add x86_64-unknown-linux-musl

RUN USER=root cargo new --bin server
WORKDIR /app

COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl
FROM alpine:latest
RUN apk add --no-cache libsfml-dev
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/server /usr/local/bin/server
CMD ["server"]
