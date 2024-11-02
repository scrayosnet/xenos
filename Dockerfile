FROM rust:alpine@sha256:466dc9924d265455aa73e72fd9cdac9db69ce6a988e6f0e6baf852db3485d97d AS builder

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
