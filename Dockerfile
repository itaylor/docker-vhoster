FROM rust:1.59 as BUILD

ARG RUSTFLAGS='-C link-arg=-s'
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
  "linux/amd64") echo x86_64-unknown-linux-musl > /rust_target.txt ;; \
  "linux/arm64") echo aarch64-unknown-linux-musl > /rust_target.txt ;; \
  *) exit 1 ;; \
esac

RUN mkdir /build && cd /build &&\
  USER=root cargo new --bin docker-vhoster &&\
  cargo install cargo-build-deps &&\
  rustup target add $(cat /rust_target.txt)
WORKDIR /build/docker-vhoster

# copy over your manifests
COPY ./Cargo.toml ./Cargo.lock ./

ENV RUSTFLAGS=${RUSTFLAGS}
# cache deps
RUN cargo build-deps --release --target=$(cat /rust_target.txt)
COPY ./src /build/docker-vhoster/src
# build as a statically linked library
RUN cargo build --release --target $(cat /rust_target.txt) --bin docker-vhoster &&\
  mv /build/docker-vhoster/target/$(cat /rust_target.txt)/release/docker-vhoster /

FROM scratch
COPY --from=BUILD /docker-vhoster /
CMD ["/docker-vhoster"]
