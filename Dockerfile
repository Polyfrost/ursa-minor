# Builder
FROM rust:1.70.0-alpine as builder

WORKDIR /usr/src/ursa-minor

RUN apk add --no-cache g++ git

COPY repo /usr/src/ursa-minor/
RUN cargo build --release

# Runner
FROM alpine:3 AS runtime

COPY --from=builder /usr/src/ursa-minor/target/release/ursa-minor /usr/local/bin

CMD ["/usr/local/bin/ursa-minor"]
