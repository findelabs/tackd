from rust:slim as builder

RUN mkdir /app 
RUN mkdir /app/bin 

COPY src /app/src/
COPY Cargo.toml /app

RUN apt-get update && apt-get install -y libssl-dev pkg-config libjemalloc-dev
ENV LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2
RUN cargo install --path /app --root /app
RUN strip app/bin/tackd

FROM debian:bullseye-slim
WORKDIR /app
COPY --from=builder /app/bin/ ./
RUN apt-get update && apt-get install -y ca-certificates

ENTRYPOINT ["/app/tackd"]
EXPOSE 8080
