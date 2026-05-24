FROM rust:1.95-alpine@sha256:606fd313a0f49743ee2a7bd49a0914bab7deedb12791f3a846a34a4711db7ed2 AS builder

# specify rust features
ARG FEATURES="default"

# specify our build directory
WORKDIR /usr/src/xenos

# copy the source files into the engine
COPY . .

# install dev dependencies and perform build process
RUN set -eux \
 && apk add --no-cache libressl-dev musl-dev protobuf-dev protoc \
 && cargo build --release --features "${FEATURES}"


FROM scratch

# declare our ports that we allow for interacting with xenos (grpc, metrics)
EXPOSE 50051 9990

# copy the raw binary into the new image
COPY --from=builder "/usr/src/xenos/target/release/xenos" "/xenos"

# copy the users and groups for the nobody user and group
COPY --from=builder "/etc/passwd" "/etc/passwd"
COPY --from=builder "/etc/group" "/etc/group"

# we run with minimum permissions as the nobody user
USER nobody:nobody

# just execute the raw binary without any wrapper
ENTRYPOINT ["/xenos"]
