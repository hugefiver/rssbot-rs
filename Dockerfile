FROM rust:alpine AS builder

WORKDIR /src


# ENV CROSS_CONTAINER_IN_CONTAINER=true
# RUN cargo install cross

RUN apk add --no-cache \
    build-base \
    musl-dev 

# ARG TARGETPLATFORM
# RUN case "${TARGETPLATFORM}" in \
#     "linux/amd64" | "linux/amd64/v[1234]" ) RUST_TARGET="x86_64-unknown-linux-musl" ;; \
#     "linux/386" ) RUST_TARGET="i586-unknown-linux-musl" ;; \
#     "linux/arm64" ) RUST_TARGET="aarch64-unknown-linux-musl" ;; \
#     "linux/arm" ) RUST_TARGET="arm-unknown-linux-musleabihf" ;; \
#     "linux/arm/v6" ) RUST_TARGET="armv6-unknown-linux-musleabihf" ;; \
#     "linux/arm/v7" ) RUST_TARGET="armv7-unknown-linux-musleabihf" ;; \ 
#     *) echo "Unsupported target: ${TARGETOS}-${TARGETARCH}" && exit 1 ;; \
#     esac; \
#     echo -n "${RUST_TARGET}" > /.triple

ARG AMD64_VERSION
ARG TARGETARCH
RUN if [ "${TARGETARCH}" == "amd64" ]; then \
    case "${AMD64_VERSION}" in \
        "v2" ) RUSTFLAGS="-C target_cpu=x86-64-v2" ;; \
        "v3" ) RUSTFLAGS="-C target_cpu=x86_64-v3" ;; \
        "v4" ) RUSTFLAGS="-C target_cpu=x86_64-v4" ;; \
    esac;  fi; \
    echo -n "${RUSTFLAGS}" > /.rustflags;

# RUN rustup toolchain add stable --profile minimal
COPY . .

ARG LOCALE=zh
ENV LOCALE=${LOCALE}

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    RUSTFLAGS="$(cat /.rustflags)" cargo build --release \
    && mkdir -p /app && cp target/release/rssbot /app/

FROM alpine:latest

COPY --from=builder /app/rssbot /rssbot

ENTRYPOINT [ "/rssbot" ]
