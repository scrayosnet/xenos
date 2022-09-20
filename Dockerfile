FROM golang@sha256:ca4f0513119dfbdc65ae7b76b69688f0723ed00d9ecf9de68abbf6ed01ef11bf AS builder

# use a separate workspace to isolate the artifacts
WORKDIR "/workspace"

# copy the go modules and manifests to download the dependencies
COPY "go.mod" "go.mod"
COPY "go.sum" "go.sum"

# cache the dependencies before copying the other source files so that this layer won't be invalidated on code changes
RUN go mod download -x

# copy all other files into the image to work on them
COPY "." "./"

# build the statically linked binary from the go source files
RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -ldflags="-w -s" -o xenos xenos.go


FROM scratch

# copy the raw binary into the new container
COPY --from=builder "/workspace/xenos" "/xenos"

# copy the users and groups for the nobody user and group
COPY --from=builder "/etc/passwd" "/etc/passwd"
COPY --from=builder "/etc/group" "/etc/group"

# we run with minimum permissions as the nobody user
USER nobody:nobody

# just execute the raw binary without any wrapper
ENTRYPOINT ["/xenos"]
