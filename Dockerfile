FROM rust:1.69-alpine AS build

RUN apk add --no-cache build-base musl-dev openssl-dev openssl

WORKDIR /usr/src/app


COPY Cargo.lock .
COPY Cargo.toml .

RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch
RUN cargo build --release
RUN rm src/main.rs

COPY src ./src/
RUN touch src/main.rs
RUN cargo build --release

FROM alpine

WORKDIR /usr/src/app
COPY --from=build /usr/src/app/target/release/dt_bigquery ./app
CMD ["./app"]
