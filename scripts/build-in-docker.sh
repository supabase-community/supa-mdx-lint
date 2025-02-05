#!/bin/bash

# Adapted from https://github.com/getsentry/sentry-cli
# See original license: https://github.com/getsentry/sentry-cli/blob/master/LICENSE

set -eux

DOCKER_IMAGE="messense/rust-musl-cross:${DOCKER_TAG}"
BUILD_DIR="/work"

DOCKER_RUN_OPTS="
  -w ${BUILD_DIR}
  -v $(pwd):${BUILD_DIR}:ro
  -v $(pwd)/target:${BUILD_DIR}/target
  -v $HOME/.cargo/registry:/root/.cargo/registry
  ${DOCKER_IMAGE}
"

docker run \
  ${DOCKER_RUN_OPTS} \
  cargo build --all-features --release --target=${TARGET} --locked

# Fix permissions for shared directories
USER_ID=$(id -u)
GROUP_ID=$(id -g)
sudo chown -R ${USER_ID}:${GROUP_ID} target/ $HOME/.cargo
