from rust
run apt-get update && apt-get -y install libfuse-dev
workdir /src
env USER root
run cargo init
copy Cargo.toml Cargo.lock ./
# run cargo build
run cargo build --release
run rm src/*.rs
copy . .
run touch src/main.rs 
# run cargo build
run cargo build --release

from ubuntu
run apt-get update && DEBIAN_FRONTEND="noninteractive" apt-get -y install \
  fuse qemu-utils \
  && apt-get clean
copy --from=0 \
  /src/target/release/fuseqemu \
  /opt/fuseqemu/
copy mount-qcow* /opt/fuseqemu/
env PATH="/opt/fuseqemu:${PATH}"
