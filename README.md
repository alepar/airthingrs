### Requirements
- `apt install libdbus-1-dev`

### Building custom cross-rs images
```shell
docker build . -f Dockerfile.cross-aarch64 -t ghcr.io/alepar/wavething-cross-rs:aarch64-unknown-linux-gnu
docker push ghcr.io/alepar/wavething-cross-rs:aarch64-unknown-linux-gnu

docker build . -f Dockerfile.cross-amd64 -t ghcr.io/alepar/wavething-cross-rs:amd64-unknown-linux-gnu
docker push ghcr.io/alepar/wavething-cross-rs:amd64-unknown-linux-gnu
```