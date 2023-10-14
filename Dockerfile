FROM rust:1.73-alpine AS build

RUN apk add --no-cache build-base musl-dev openssl-dev openssl ca-certificates

WORKDIR /usr/src/app

COPY Cargo.lock .
COPY Cargo.toml .

RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

COPY src ./src/
RUN touch src/main.rs
RUN cargo build --release

FROM scratch
WORKDIR /usr/src/app
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /usr/src/app/target/release/dt_bigquery ./app
CMD ["./app"]
