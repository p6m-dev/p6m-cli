FROM rust:1.73-alpine3.18 as builder

WORKDIR /app

RUN apk add build-base git

RUN apk add pkgconfig openssl-dev

COPY . .

RUN cargo build --release


FROM alpine:3.18

COPY --from=builder /app/target/release/ybor /ybor

ENTRYPOINT ["/ybor"]
