FROM rust:latest AS builder
RUN apt-get update && apt-get install -y \
    libsfml-dev \
    && rm -rf /var/lib/apt/lists/*
RUN USER=root cargo new --bin server
WORKDIR /app
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y \
    libsfml-dev \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/server /usr/local/bin/server

CMD ["server"]
