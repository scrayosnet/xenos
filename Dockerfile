FROM rust:alpine@sha256:ec93a9ad3065df593645171a3aa6c47b55578914d2c232860260dbd27bb0cbc0 AS builder

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

# copy the raw binary into the new image
COPY --from=builder "/usr/src/xenos/target/release/xenos" "/xenos"

# copy the config into the new image
COPY "./config" "/config"

# copy the users and groups for the nobody user and group
COPY --from=builder "/etc/passwd" "/etc/passwd"
COPY --from=builder "/etc/group" "/etc/group"

# we run with minimum permissions as the nobody user
USER nobody:nobody

# just execute the raw binary without any wrapper
ENTRYPOINT ["/xenos"]
