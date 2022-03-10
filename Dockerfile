FROM rust:1.59 as BUILD

ARG RUSTFLAGS='-C link-arg=-s'
ARG TARGET='x86_64-unknown-linux-musl'

RUN USER=root cargo new --bin docker-vhoster
WORKDIR /docker-vhoster

# copy over your manifests
COPY ./Cargo.toml ./Cargo.toml

ENV RUSTFLAGS=${RUSTFLAGS}
# cache deps
RUN rustup target add ${TARGET} && cargo build --release --target ${TARGET} && rm src/*.rs
COPY ./src /docker-vhoster/src
# build as a statically linked library
RUN cargo build --release --target ${TARGET} --bin docker-vhoster
#RUN ls -alh /docker-vhoster/target/${TARGET} && exit 1  

FROM scratch
ARG TARGET='x86_64-unknown-linux-musl'
COPY --from=BUILD /docker-vhoster/target/${TARGET}/release/docker-vhoster /
CMD ["/docker-vhoster"]
