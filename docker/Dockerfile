FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY card/ card/
COPY proto/ proto/
COPY node/ node/
COPY validator/ validator/
COPY publisher/ publisher/
COPY consumer/ consumer/
COPY verifier/ verifier/
RUN cargo build --release --bin p2s-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget iproute2 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/p2s-node /usr/local/bin/p2s-node
RUN mkdir -p /data
ENV RUST_LOG=p2s_node=info,libp2p=warn
EXPOSE 4001 8080
ENTRYPOINT ["p2s-node"]
CMD ["--listen", "/ip4/0.0.0.0/tcp/4001", "--http-port", "8080", "--data-dir", "/data"]
