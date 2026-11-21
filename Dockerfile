# Build from source so `docker build` works without a prebuilt binary in the
# build context. The published multi-arch image uses the same binary.
FROM rust:1.89-alpine AS builder

RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM alpine:3.21

RUN apk add --no-cache ca-certificates \
    && addgroup -S app \
    && adduser -S -G app -h /home/app app
COPY --from=builder /src/target/release/hkpollution /usr/local/bin/hkpollution
COPY LICENSE /LICENSE

USER app
ENTRYPOINT ["/usr/local/bin/hkpollution"]
CMD ["--help"]
