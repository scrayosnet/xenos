FROM rust:alpine@sha256:9af4ed962405e24b37240ce34a2272e40cff99b4f5150cc6a53b03f95d40e6e0 AS builder

# specify our build directory
WORKDIR /usr/src/xenos

# copy the source files into the engine
COPY . .

# install dev dependencies and perform build process
RUN set -eux \
 && apk add --no-cache musl-dev protoc protobuf-dev libressl-dev \
 && cargo build --release


FROM scratch

# declare our ports that we allow for interacting with xenos (grpc, metrics)
EXPOSE 50051 9990

# copy the raw binary into the new container
COPY --from=builder "/usr/src/xenos/target/release/xenos" "/xenos"

# copy the users and groups for the nobody user and group
COPY --from=builder "/etc/passwd" "/etc/passwd"
COPY --from=builder "/etc/group" "/etc/group"

# we run with minimum permissions as the nobody user
USER nobody:nobody

# just execute the raw binary without any wrapper
ENTRYPOINT ["/xenos"]
