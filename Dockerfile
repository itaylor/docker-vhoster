FROM rust:1.60 AS chef

RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS PLANNER
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS BUILD
COPY --from=PLANNER /app/recipe.json /build/docker-vhoster/recipe.json

ARG RUSTFLAGS='-C link-arg=-s'
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
  "linux/amd64") echo x86_64-unknown-linux-musl > /rust_target.txt ;; \
  "linux/arm64") echo aarch64-unknown-linux-musl > /rust_target.txt ;; \
  *) exit 1 ;; \
esac

WORKDIR /build/docker-vhoster

# Install the target arch
RUN rustup target add $(cat /rust_target.txt)

# cache the deps with cargo chef
RUN cargo chef cook --release --target $(cat /rust_target.txt) --recipe-path recipe.json

ENV RUSTFLAGS=${RUSTFLAGS}

# Now we copy the source code...
COPY . /build/docker-vhoster
# build as a statically linked library
RUN cargo build --release --target $(cat /rust_target.txt) --bin docker-vhoster &&\
  mv /build/docker-vhoster/target/$(cat /rust_target.txt)/release/docker-vhoster /

FROM scratch
COPY --from=BUILD /docker-vhoster /
CMD ["/docker-vhoster"]
