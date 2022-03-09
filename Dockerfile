FROM rust:1.59 as BUILD

RUN USER=root cargo new --bin docker-vhoster
WORKDIR /docker-vhoster

# copy over your manifests
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN cargo build --release && rm src/*.rs
COPY ./src /docker-vhoster/src
RUN cargo build --release --bin 

# our final base
FROM debian:bullseye-slim
# copy the build artifact from the build stage
COPY --from=BUILD docker-vhoster .
# set the startup command to run your binary
CMD ["./docker-vhoster"]