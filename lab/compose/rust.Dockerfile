FROM rust:1.94-bookworm AS build
WORKDIR /src
ENV CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
COPY backend-rs/Cargo.toml backend-rs/Cargo.lock ./
COPY backend-rs/src ./src
RUN cargo test --locked && cargo build --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/debug/podobank-api /usr/local/bin/podobank-api
USER 65534:65534
ENTRYPOINT ["podobank-api", "--host", "0.0.0.0", "--port", "8000"]
