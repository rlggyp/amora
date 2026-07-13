FROM rust:bookworm AS builder

WORKDIR /app
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release
RUN mkdir -p /etc/amora/{configs,logs}

FROM gcr.io/distroless/cc
COPY --from=builder /app/target/release/amora /amora
COPY --from=builder /etc/amora /etc/amora

ENV TZ=Asia/Jakarta
EXPOSE 12013
USER nonroot
ENTRYPOINT ["/amora"]