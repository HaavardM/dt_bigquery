FROM rust:1.57-alpine AS build

RUN apk add --no-cache alpine-sdk

WORKDIR /usr/src/app


COPY Cargo.lock .
COPY Cargo.toml .

RUN rm -rf src

COPY . .

RUN cargo install --path .

FROM alpine

WORKDIR /usr/src/app
COPY --from=build /usr/src/app/target/release/dt_bigquery ./app
CMD ["./app"]
