FROM rust:alpine3.19 as builder

WORKDIR /usr/src/haimdall

COPY . .

RUN cargo install --path .

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/cargo/bin/haimdall /usr/local/bin/haimdall

EXPOSE 50051

CMD ["haimdall"]