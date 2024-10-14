FROM rust:latest AS builder
RUN apt-get update && apt-get install -y \
    libsfml-dev \
    && rm -rf /var/lib/apt/lists/*
RUN USER=root cargo new --bin server
WORKDIR /app
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    libsfml-dev \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/server /usr/local/bin/server
RUN echo "0" > /app/bots.txt
WORKDIR /app
CMD ["server"]
